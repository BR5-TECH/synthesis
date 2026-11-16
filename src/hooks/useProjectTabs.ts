import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../api";
import { getWindowProjectId, setWindowProjectId } from "../api/invoke";
import {
  onMenuEvent,
  onProjectWindowCloseRequested,
  onProjectWindowsChanged,
  MENU_NEW_TAB,
} from "../events";
import { logDebug, logError, logInfo, logWarn } from "../logging";
import { loadAppPreferences, patchAppPreferences } from "../state/appPreferences";
import type {
  OpenProjectBehaviour,
  Operation,
  ProjectHandle,
  ProjectWindow,
  ProjectWindowsSnapshot,
} from "../types";
import type { OpenProjectChoice } from "../components/OpenProjectDialog";
import type { ProjectWindowBoot } from "../projectWindowBoot";

/**
 * PJT-FR-PBQT: on macOS the system draws the tab bar, so the app draws none.
 * PWN-FR-XGTO: a macOS window shares the frame of its tab group.
 */
export function isMacPlatform(): boolean {
  return typeof navigator !== "undefined" && /mac/i.test(navigator.platform);
}

/**
 * PJT-FR-OJZG: the operation kinds that count as a run, which are an agent
 * turn and a graduation run. Indexing, embeddings jobs, searches and stream
 * updates are operations too, but a close does not ask about them.
 */
const RUN_OPERATION_KINDS: ReadonlySet<string> = new Set(["agent", "graduation"]);

/** How many of the operations in flight are a run that a close would stop. */
export function countRunsInFlight(operations: readonly Operation[]): number {
  return operations.filter((o) => RUN_OPERATION_KINDS.has(o.kind)).length;
}

export interface ProjectTabsDeps {
  /** The screen of this window: the Project picker, or a project. */
  screen: "picker" | "ide";
  /** Mount the shell for a project that the backend opened (OVW-FR-04). */
  loadProject: (handle: ProjectHandle) => void;
  /** EDT-FR-33: write pending changes; `false` cancels. */
  flushBeforeTeardown: () => Promise<boolean>;
  /** PJT-FR-PEIJ: write, tear down, and close the project tab of this window. */
  requestCloseProject: () => Promise<void>;
  /** The runs in flight in this window's project: see {@link countRunsInFlight} (PJT-FR-OJZG). */
  runningOperations: number;
  /** PWN-FR-KQZB: set when "open project window" created this window. */
  boot?: ProjectWindowBoot | null;
  /** PWN-FR-KQZB: close this OS window. */
  closeWindow?: () => Promise<void>;
}

/**
 * PJT-FR-IOEV / PJT-FR-NWTD: what opened the Project picker modal. New Tab has
 * made the choice already; Open Another Project… has not.
 */
export type PickerIntent = "ask" | "new-tab";

/** PWN-FR-KQZB: the loading state of a window that mounts its project. */
export interface BootState {
  name: string;
  /** Empty while the mount runs; the failure once it failed. */
  error: string;
}

/**
 * PWN-FR-XGTO / PJT-FR-QFRM: whether a window restores its stored size and
 * full-screen state. A window in a macOS tab group shares the frame of the
 * group, so it restores nothing.
 */
export function restoresGeometry(window: {
  created: boolean;
  mac: boolean;
  inTabGroup: boolean;
}): boolean {
  return !((window.created && window.mac) || window.inTabGroup);
}

/** The stored answer of the open project dialog for one choice (PJT-FR-RMBR). */
function behaviourOf(choice: OpenProjectChoice): OpenProjectBehaviour {
  return choice === "new-tab" ? "new_tab" : "this_tab";
}

export interface ProjectTabs {
  /** The tabs the in-app strip draws; empty on macOS (PJT-FR-PBQT). */
  stripWindows: ProjectWindow[];
  ownProjectId: string;
  /** PJT-FR-IOEV: the Project picker modal shows. */
  pickerOpen: boolean;
  /** PJT-FR-NWTD: what opened the modal. */
  pickerIntent: PickerIntent;
  openPicker: () => void;
  closePicker: () => void;
  /** PJT-FR-PNDG: the name of the project that opens in a new tab now; empty when none. */
  opening: string;
  /** PJT-FR-GASB: the project the open project dialog asks about. */
  pending: ProjectHandle | null;
  chooseProject: (handle: ProjectHandle) => Promise<void>;
  answerDialog: (choice: OpenProjectChoice, remember?: boolean) => Promise<void>;
  cancelDialog: () => void;
  /** PJT-FR-OJZG: the close confirmation shows. */
  confirmingClose: boolean;
  confirmClose: () => Promise<void>;
  cancelClose: () => void;
  /** PJT-FR-PCCL: close the project tab of this window. */
  requestClose: () => Promise<void>;
  activateTab: (projectId: string) => void;
  closeTab: (projectId: string) => void;
  /** The last failure of an open, as text; empty when none. */
  error: string;
  /** PWN-FR-KQZB: the loading state of this window; `null` once it holds its project. */
  boot: BootState | null;
  /** PWN-FR-KQZB: close this window after its mount failed. */
  closeBootWindow: () => Promise<void>;
  /** PJT-FR-QFRM: this window shares a macOS tab group with other project windows. */
  inTabGroup: () => boolean;
}

/**
 * The project tab model of this window (`PJT-project-tabs.md`).
 *
 * The window is the project tab: it holds one project, and the backend keeps the
 * list of all project windows. This hook mounts the project of the window, asks
 * where a chosen project opens, runs the close flow, and follows the list.
 */
export function useProjectTabs(deps: ProjectTabsDeps): ProjectTabs {
  const depsRef = useRef(deps);
  depsRef.current = deps;
  const [snapshot, setSnapshot] = useState<ProjectWindowsSnapshot>({
    windows: [],
    activeProjectId: null,
  });
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerIntent, setPickerIntent] = useState<PickerIntent>("ask");
  const pickerRef = useRef({ open: false, intent: "ask" as PickerIntent });
  pickerRef.current = { open: pickerOpen, intent: pickerIntent };
  const [opening, setOpening] = useState("");
  const openingRef = useRef(opening);
  openingRef.current = opening;
  // PJT-FR-PNDG: set while a choice is decided, before its open starts, so a
  // second choice in that time is ignored too.
  const choosingRef = useRef(false);
  const [boot, setBoot] = useState<BootState | null>(() =>
    deps.boot ? { name: deps.boot.name, error: "" } : null,
  );
  const bootProjectRef = useRef<string | null>(null);
  const [pending, setPending] = useState<ProjectHandle | null>(null);
  const pendingRef = useRef(pending);
  pendingRef.current = pending;
  const [confirmingClose, setConfirmingClose] = useState(false);
  const [error, setError] = useState("");
  const snapshotRef = useRef(snapshot);
  snapshotRef.current = snapshot;

  // PWN-FR-NEYF: follow the list. The subscription comes first, so a change that
  // lands during the first read is not lost.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onProjectWindowsChanged((next) => {
      if (!cancelled) setSnapshot(next);
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    void api
      .listProjectWindows()
      .then((initial) => {
        if (!cancelled && initial && Array.isArray(initial.windows)) setSnapshot(initial);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  /** Open a project in this window: its scope, its mount, its entry in the list. */
  const mountHere = useCallback(async (handle: ProjectHandle) => {
    const previous = getWindowProjectId();
    setWindowProjectId(handle.path);
    try {
      const mounted = await api.openProjectAtPath(handle.path);
      try {
        await api.attachProjectToWindow(mounted.path);
      } catch (e) {
        // Another window holds the project: this window must not show it too.
        if (String(e).includes("project_already_open")) throw e;
        // Any other failure leaves the project open here, only unlisted.
        logWarn(["frontend"], "project window could not be listed", { error: String(e) });
      }
      depsRef.current.loadProject(mounted);
    } catch (e) {
      setWindowProjectId(previous);
      throw e;
    }
  }, []);

  // A window that `open project window` created holds its project already in
  // the list: it mounts that project at startup (PJT-FR-JLHP).
  useEffect(() => {
    let cancelled = false;
    const booting = !!depsRef.current.boot;
    void api
      .currentProjectWindow()
      .then(async (entry) => {
        if (cancelled || depsRef.current.screen !== "picker") return;
        if (!entry) {
          if (!booting) return;
          // PWN-FR-KQZB: the list holds no project for this window any more. It
          // closes, so it never shows the Project picker inside a tab group.
          // The only window of the application shows the picker instead.
          logWarn(["frontend"], "created project window holds no project");
          const others = await api.listProjectWindows().catch(() => null);
          const close = depsRef.current.closeWindow;
          if (others && others.windows.length > 0 && close) {
            // A close that throws at once fails the same way as a rejected one.
            await Promise.resolve()
              .then(close)
              .catch((e) => {
                logError(["frontend"], "created project window could not close", {
                  error: String(e),
                });
                setBoot(null);
              });
            return;
          }
          if (!cancelled) setBoot(null);
          return;
        }
        bootProjectRef.current = entry.projectId;
        logInfo(["frontend"], "project window mounts its project");
        await mountHere({
          name: entry.name,
          path: entry.projectId,
          activeWorktreePath: entry.projectId,
          rememberedWorktreeUnavailable: false,
        });
        if (!cancelled) setBoot(null);
      })
      .catch((e) => {
        if (cancelled) return;
        logError(["frontend"], "project window could not mount its project", {
          error: String(e),
        });
        // PWN-FR-KQZB: the window shows the failure; the user closes it.
        if (booting) setBoot((b) => (b ? { ...b, error: String(e) } : b));
      });
    return () => {
      cancelled = true;
    };
  }, [mountHere]);

  // PJT-FR-IOEV: Open Another Project… asks where the project opens.
  const openPicker = useCallback(() => {
    setPickerIntent("ask");
    setPickerOpen(true);
  }, []);
  const closePicker = useCallback(() => {
    setPickerOpen(false);
    setPickerIntent("ask");
  }, []);

  // PJT-FR-PILG / PJT-FR-NWTD: New Tab opens the Project picker over this window,
  // and the project chosen in it opens in a new tab.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onMenuEvent(MENU_NEW_TAB, () => {
      if (depsRef.current.screen !== "ide") return;
      // PJT-FR-PILG: the modal, the open project dialog, or an open in progress
      // holds the window; New Tab changes nothing then.
      const busy =
        pickerRef.current.open ||
        pendingRef.current !== null ||
        openingRef.current !== "" ||
        choosingRef.current;
      if (busy) {
        logDebug(["frontend"], "new tab ignored while a project choice is open");
        return;
      }
      setPickerIntent("new-tab");
      setPickerOpen(true);
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  /** PJT-FR-PNDG / PJT-FR-JLHP: open a project in a new tab, with the modal busy. */
  const openInNewTab = useCallback(async (handle: ProjectHandle) => {
    openingRef.current = handle.name;
    setOpening(handle.name);
    logInfo(["frontend"], "project opens in a new tab");
    try {
      await api.openProjectWindow(handle.path);
      setPickerOpen(false);
      setPickerIntent("ask");
    } catch (e) {
      setError(String(e));
      logError(["frontend"], "project could not be opened in a new tab", { error: String(e) });
    } finally {
      setOpening("");
    }
  }, []);

  /** PJT-FR-ERZZ: replace the project of this window. */
  const replaceHere = useCallback(
    async (handle: ProjectHandle) => {
      try {
        // OVW-FR-11: pending changes are written first. A write that cannot
        // proceed cancels the replacement.
        if (!(await depsRef.current.flushBeforeTeardown())) return;
        await api.closeProject().catch((e) => {
          logWarn(["frontend"], "project teardown failed", { error: String(e) });
        });
        await mountHere(handle);
        setPickerOpen(false);
        setPickerIntent("ask");
      } catch (e) {
        setError(String(e));
        logError(["frontend"], "project could not be opened", { error: String(e) });
      }
    },
    [mountHere],
  );

  /** PJT-FR-ZRZD: a project that a tab holds already is activated, not opened again. */
  const activateIfOpen = useCallback((handle: ProjectHandle): boolean => {
    if (!snapshotRef.current.windows.some((w) => w.projectId === handle.path)) return false;
    setPickerOpen(false);
    setPickerIntent("ask");
    setPending(null);
    void api.activateProjectWindow(handle.path).catch(() => {});
    return true;
  }, []);

  const chooseProject = useCallback(
    async (handle: ProjectHandle) => {
      // PJT-FR-PNDG: one open at a time.
      if (openingRef.current || choosingRef.current) return;
      setError("");
      if (activateIfOpen(handle)) return;
      if (depsRef.current.screen === "picker") {
        try {
          await mountHere(handle);
        } catch (e) {
          setError(String(e));
          logError(["frontend"], "project could not be opened", { error: String(e) });
        }
        return;
      }
      choosingRef.current = true;
      try {
        // PJT-FR-NWTD: New Tab has made the choice already.
        const picker = pickerRef.current;
        if (picker.open && picker.intent === "new-tab") {
          await openInNewTab(handle);
          return;
        }
        // PJT-FR-RMBR: a remembered choice runs with no dialog.
        const behaviour = (await loadAppPreferences()).openProjectBehaviour ?? "ask";
        if (behaviour === "new_tab") {
          await openInNewTab(handle);
        } else if (behaviour === "this_tab") {
          await replaceHere(handle);
        } else {
          setPending(handle);
        }
      } finally {
        choosingRef.current = false;
      }
    },
    [mountHere, activateIfOpen, openInNewTab, replaceHere],
  );

  const answerDialog = useCallback(
    async (choice: OpenProjectChoice, remember = false) => {
      const handle = pending;
      if (!handle) return;
      setPending(null);
      setError("");
      // PJT-FR-RMBR: the choice is stored before the project opens. A failed
      // write does not stop the open.
      if (remember) {
        await patchAppPreferences({ openProjectBehaviour: behaviourOf(choice) }).catch((e) => {
          logWarn(["frontend"], "open project choice not remembered", { error: String(e) });
        });
      }
      if (activateIfOpen(handle)) return;
      // PJT-FR-PILG / PJT-FR-PNDG: a This tab replace is an open in progress too.
      choosingRef.current = true;
      try {
        if (choice === "new-tab") await openInNewTab(handle);
        else await replaceHere(handle);
      } finally {
        choosingRef.current = false;
      }
    },
    [pending, activateIfOpen, openInNewTab, replaceHere],
  );

  const cancelDialog = useCallback(() => setPending(null), []);

  // PJT-FR-OJZG: a run in flight asks first.
  const requestClose = useCallback(async () => {
    if (depsRef.current.runningOperations > 0) {
      setConfirmingClose(true);
      return;
    }
    await depsRef.current.requestCloseProject();
  }, []);

  const confirmClose = useCallback(async () => {
    setConfirmingClose(false);
    await depsRef.current.requestCloseProject();
  }, []);

  const cancelClose = useCallback(() => setConfirmingClose(false), []);

  // PWN-FR-BYCL: another window asked this one to close its project tab.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onProjectWindowCloseRequested((projectId) => {
      if (projectId === getWindowProjectId()) void requestClose();
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [requestClose]);

  const ownProjectId = getWindowProjectId() ?? "";

  // PJT-FR-QFRM: read when a restore would run, so it sees the list of now.
  const inTabGroup = useCallback(
    () => isMacPlatform() && snapshotRef.current.windows.length > 1,
    [],
  );

  // PWN-FR-KQZB: a window whose mount failed closes itself and its entry.
  const closeBootWindow = useCallback(async () => {
    const projectId = bootProjectRef.current;
    if (!projectId) {
      setBoot(null);
      return;
    }
    try {
      const result = await api.closeProjectWindow(projectId);
      // The last window stays and shows the Project picker (PWN-FR-QEGI).
      if (result?.lastClosed) setBoot(null);
    } catch (e) {
      logError(["frontend"], "project window could not be closed", { error: String(e) });
      setBoot(null);
    }
  }, []);

  const activateTab = useCallback((projectId: string) => {
    void api.activateProjectWindow(projectId).catch(() => {});
  }, []);

  const closeTab = useCallback(
    (projectId: string) => {
      if (projectId === getWindowProjectId()) {
        void requestClose();
        return;
      }
      // PJT-FR-MWLY: a close that may need a confirmation first activates its
      // tab, so the dialog is in front.
      void api
        .activateProjectWindow(projectId)
        .then(() => api.requestProjectWindowClose(projectId))
        .catch(() => {});
    },
    [requestClose],
  );

  return {
    stripWindows: isMacPlatform() ? [] : snapshot.windows,
    ownProjectId,
    pickerOpen,
    pickerIntent,
    openPicker,
    closePicker,
    opening,
    pending,
    chooseProject,
    answerDialog,
    cancelDialog,
    confirmingClose,
    confirmClose,
    cancelClose,
    requestClose,
    activateTab,
    closeTab,
    error,
    boot,
    closeBootWindow,
    inTabGroup,
  };
}

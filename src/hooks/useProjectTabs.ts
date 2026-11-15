import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../api";
import { getWindowProjectId, setWindowProjectId } from "../api/invoke";
import {
  onMenuEvent,
  onProjectWindowCloseRequested,
  onProjectWindowsChanged,
  MENU_NEW_TAB,
} from "../events";
import { logError, logInfo, logWarn } from "../logging";
import type {
  Operation,
  ProjectHandle,
  ProjectWindow,
  ProjectWindowsSnapshot,
} from "../types";
import type { OpenProjectChoice } from "../components/OpenProjectDialog";

/** PJT-FR-PBQT: on macOS the system draws the tab bar, so the app draws none. */
function isMacPlatform(): boolean {
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
}

export interface ProjectTabs {
  /** The tabs the in-app strip draws; empty on macOS. */
  stripWindows: ProjectWindow[];
  ownProjectId: string;
  /** PJT-FR-IOEV: the Project picker modal shows. */
  pickerOpen: boolean;
  openPicker: () => void;
  closePicker: () => void;
  /** PJT-FR-GASB: the project the open project dialog asks about. */
  pending: ProjectHandle | null;
  chooseProject: (handle: ProjectHandle) => Promise<void>;
  answerDialog: (choice: OpenProjectChoice) => Promise<void>;
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
  const [pending, setPending] = useState<ProjectHandle | null>(null);
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
    void api
      .currentProjectWindow()
      .then(async (entry) => {
        if (cancelled || !entry || depsRef.current.screen !== "picker") return;
        logInfo(["frontend"], "project window mounts its project");
        await mountHere({
          name: entry.name,
          path: entry.projectId,
          activeWorktreePath: entry.projectId,
          rememberedWorktreeUnavailable: false,
        });
      })
      .catch((e) => {
        if (!cancelled) {
          logError(["frontend"], "project window could not mount its project", {
            error: String(e),
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [mountHere]);

  const openPicker = useCallback(() => setPickerOpen(true), []);
  const closePicker = useCallback(() => setPickerOpen(false), []);

  // PJT-FR-PILG: New Tab opens the Project picker over this window.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onMenuEvent(MENU_NEW_TAB, () => {
      if (depsRef.current.screen === "ide") setPickerOpen(true);
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

  const chooseProject = useCallback(
    async (handle: ProjectHandle) => {
      setError("");
      // PJT-FR-ZRZD: a project that a tab holds already is activated, not opened
      // a second time.
      if (snapshotRef.current.windows.some((w) => w.projectId === handle.path)) {
        setPickerOpen(false);
        setPending(null);
        await api.activateProjectWindow(handle.path).catch(() => {});
        return;
      }
      if (depsRef.current.screen === "picker") {
        try {
          await mountHere(handle);
        } catch (e) {
          setError(String(e));
          logError(["frontend"], "project could not be opened", { error: String(e) });
        }
        return;
      }
      setPending(handle);
    },
    [mountHere],
  );

  const answerDialog = useCallback(
    async (choice: OpenProjectChoice) => {
      const handle = pending;
      if (!handle) return;
      setPending(null);
      setError("");
      if (snapshotRef.current.windows.some((w) => w.projectId === handle.path)) {
        setPickerOpen(false);
        await api.activateProjectWindow(handle.path).catch(() => {});
        return;
      }
      try {
        if (choice === "new-tab") {
          await api.openProjectWindow(handle.path);
        } else {
          // PJT-FR-ERZZ / OVW-FR-11: pending changes are written first. A write
          // that cannot proceed cancels the replacement.
          if (!(await depsRef.current.flushBeforeTeardown())) return;
          await api.closeProject().catch((e) => {
            logWarn(["frontend"], "project teardown failed", { error: String(e) });
          });
          await mountHere(handle);
        }
        setPickerOpen(false);
      } catch (e) {
        setError(String(e));
        logError(["frontend"], "project could not be opened", { error: String(e) });
      }
    },
    [pending, mountHere],
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
    openPicker,
    closePicker,
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
  };
}

import "./styles/colors_and_type.css";
import "./styles/components.css";
import "./styles/kit.css";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import * as api from "./api";
import { SettingsBody } from "./components/SettingsBody";
import { GithubTokenPicker } from "./components/GithubTokenPicker";
import { DiagramEditorHost } from "./components/DiagramEditor";
import { useThemePreference } from "./hooks/useThemePreference";
import { useFontRoles } from "./hooks/useFontRoles";
import { useLineEndings } from "./hooks/useLineEndings";
import {
  onSettingsPresentFailure,
  onSettingsRoute,
  onSettingsSaveAndClose,
} from "./events";
import { logError, logInfo, logWarn } from "./logging";
import { runSettingsSaveSweep, type SectionRequest } from "./state/settingsSweep";
import {
  NEW_AGENT_ITEM,
  parseSectionAddress,
  type SettingsScope,
  type SettingsWindowContext,
} from "./settingsWindow";
import type { AgentEditorRequest } from "./components/Agents";

/**
 * The root of the Settings **child window**
 * (`../specifications/ui/SWN-settings-windows.md`).
 *
 * Global settings and Project settings are the two groups of one native child
 * window rather than tabs of the main viewport (SWN-FR-01), and the window is a
 * webview of its own. This is what that webview renders: the one side nav with
 * its two groups and the section on screen, and the things the window owes the
 * rest of the application — the save sweep it runs before it goes (SWN-FR-08)
 * and the section it presents when asked to (SWN-FR-HZWD) or when a save fails
 * (SWN-FR-11).
 *
 * Nothing of the main window's chrome is drawn here: no tab strip, no activity
 * bar, no status bar. The frame is the platform's own, and its size, position,
 * modality, and title all belong to the backend (`settings_window.rs`).
 */
export function SettingsWindowApp({
  initialSection,
}: {
  /** SWN-FR-HZWD: the section address the request that opened the window named. */
  initialSection: string | null;
}) {
  // OVW-FR-13 / OVW-FR-14: the window renders in the current theme and the
  // three typographic roles exactly as the main window does — a settings window
  // is a separate window rather than a separately-styled application.
  const { themePref, setThemePref } = useThemePreference();
  const { fonts, setFonts } = useFontRoles();

  /**
   * SWN-FR-02: the open project behind this window.
   *
   * Read once, because it cannot change while the window is open — the parent
   * is blocked, so no project close, project switch, or worktree change can
   * start (SET-FR-20). `undefined` while the read is outstanding, `null` when it
   * failed, which reads as no project.
   */
  const [context, setContext] = useState<SettingsWindowContext | null | undefined>(
    undefined,
  );
  useEffect(() => {
    let cancelled = false;
    void api
      .getSettingsWindowContext()
      .then((ctx) => {
        if (!cancelled) setContext(ctx);
      })
      .catch((e) => {
        // The window still renders: it is reachable with no project at all
        // (SWN-FR-15), and then offers its Global group alone (SWN-FR-16).
        if (!cancelled) setContext(null);
        logWarn(["frontend"], "settings window could not read its project", {
          reason: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /**
   * SWN-FR-16: the Project group is present only while a project is open. Read
   * from the context, because the window is a webview of its own and cannot see
   * the main window's state.
   */
  const projectOpen = Boolean(context?.projectKey);
  const scopes = useMemo<SettingsScope[]>(
    () => (projectOpen ? ["global", "project"] : ["global"]),
    [projectOpen],
  );

  /**
   * SWN-FR-HZWD / SWN-FR-11: the section address to present, seeded from the request that
   * opened the window and replaced by every later route or failure.
   */
  const [sectionRequest, setSectionRequest] = useState<SectionRequest | null>(
    initialSection ? { section: initialSection, nonce: 0 } : null,
  );
  const presentSection = useCallback((section: string | null) => {
    if (!section) return;
    setSectionRequest((prev) => ({
      section,
      nonce: (prev?.nonce ?? 0) + 1,
    }));
  }, []);

  /**
   * AGT-FR-06 / AGT-FR-07: the persona editor a route asked for, and the group
   * whose Agents section it belongs to.
   *
   * Derived from the section address rather than carried beside it, so the
   * chrome roster's two actions travel the same one-string route every other
   * sectioned request does — `agents:new` for a fresh persona, `agents:<id>`
   * for one that exists, and a bare `agents` for the section alone. The scope
   * of the address picks the section: `global` is the Personal Agents section
   * and `project` is the Project Agents section.
   */
  const [honouredEditor, setHonouredEditor] = useState<number | null>(null);
  const agentEditor = useMemo<{
    scope: SettingsScope;
    request: AgentEditorRequest;
  } | null>(() => {
    if (!sectionRequest) return null;
    // Honoured once: without this the section would reopen the editor every
    // time its agent list changed underneath it — which is every create, every
    // edit, and every delete the author makes while it is open.
    if (honouredEditor === sectionRequest.nonce) return null;
    const parsed = parseSectionAddress(sectionRequest.section);
    if (!parsed) return null;
    const { section, item } = parsed;
    if (section !== "agents" || item === null) return null;
    return {
      scope: parsed.scope,
      request: {
        agentId: item === NEW_AGENT_ITEM ? null : item,
        nonce: sectionRequest.nonce,
      },
    };
    // Memoised on the request itself rather than rebuilt per render: the
    // section watches this object's identity, and a fresh one each render would
    // reopen the editor every time anything else in the window changed.
  }, [sectionRequest, honouredEditor]);
  const agentEditorRequest =
    agentEditor?.scope === "global" ? agentEditor.request : null;
  const projectAgentEditorRequest =
    agentEditor?.scope === "project" ? agentEditor.request : null;
  const handleAgentEditorRequest = useCallback(
    () => setHonouredEditor(sectionRequest?.nonce ?? null),
    [sectionRequest],
  );

  useEffect(() => {
    let cancelled = false;
    const unlisten: Array<() => void> = [];
    const keep = (fn: () => void) => {
      if (cancelled) fn();
      else unlisten.push(fn);
    };
    // SWN-FR-06 / SWN-FR-HZWD: a request for the window already open focuses it
    // and presents the section it named. The window is not reloaded, so its
    // sections keep their state.
    void onSettingsRoute(presentSection).then(keep);
    // SWN-FR-11: the failed section is the section presented, so the error and
    // the retry are visible rather than behind a section the author is not on.
    void onSettingsPresentFailure(presentSection).then(keep);
    return () => {
      cancelled = true;
      unlisten.forEach((fn) => fn());
    };
  }, [presentSection]);

  /**
   * SWN-FR-08 through SWN-FR-12: the save-before-close sweep.
   *
   * Every close — the author's own and the one a quit performs first — reaches
   * this window as one request, and this is the whole of the answer: write every
   * pending change in every section of both groups, then report. No discard prompt exists anywhere in it (SWN-FR-10), because the
   * close writes what would otherwise be discarded.
   */
  const sweeping = useRef(false);
  /**
   * Answer the sweep, and say so when the answer cannot be delivered.
   *
   * An undelivered answer is the one failure this window cannot recover from on
   * its own: the backend goes on holding the sweep, every later request is inert
   * (SWN-FR-12), and the parent stays blocked behind a window that will not
   * close (SWN-FR-02). Nothing here can retry it usefully — the bridge is what
   * failed — so the record is the whole of what it can do, and without one the
   * wedge has no explanation anywhere.
   */
  const answer = useCallback(
    async (ok: boolean, section: string | null) => {
      try {
        await api.finishSettingsClose(ok, section);
      } catch (e) {
        logError(["frontend"], "settings window could not answer its save sweep", {
          ok,
          reason: e instanceof Error ? e.message : String(e),
        });
      }
    },
    [],
  );
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onSettingsSaveAndClose(() => {
      // SWN-FR-12: a second request arriving while the sweep runs starts no
      // further save and duplicates no write already in flight. The backend
      // holds the same rule; this is the half that owns the writes.
      if (sweeping.current) return;
      sweeping.current = true;
      void runSettingsSaveSweep()
        .then((result) => {
          if (result.ok) {
            logInfo(["frontend"], "settings window saved its pending changes", {});
            return answer(true, null);
          }
          // SWN-FR-11: the transition is cancelled, the window stays open, and
          // the failed section keeps the author's edits exactly as they were.
          logWarn(["frontend"], "settings window save failed; close cancelled", {
            section: result.section,
          });
          presentSection(result.section);
          return answer(false, result.section);
        })
        .finally(() => {
          sweeping.current = false;
        });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [answer, presentSection]);

  const { lineEndings, selectLineEndings } = useLineEndings(
    context?.contentRoot ?? "",
  );

  /**
   * GHA-FR-20 / GHA-FR-21: the token picker, opened from the Project section
   * with nothing waiting on it. One instance for the window, exactly as the main
   * window holds one for itself, so it cannot coexist with a second. It is
   * presented **within this window** rather than over the main one — that window
   * being modal to this one and unable to show anything.
   */
  const [picker, setPicker] = useState<{
    currentTokenId: string | null;
    onConfirmed: () => void;
  } | null>(null);

  // The body waits for the project question to be answered, so the Project
  // group is never seen to appear after the window is already on screen
  // (SWN-FR-16, and the window being at its final state before it is
  // interactive).
  if (context === undefined) {
    return <div className="app settings-window" />;
  }

  return (
    <div className="app settings-window">
      <SettingsBody
        scopes={scopes}
        sectionRequest={sectionRequest}
        autoFocus
        global={{
          agentEditorRequest,
          onAgentEditorRequestHandled: handleAgentEditorRequest,
          controlledTheme: themePref,
          onThemeChange: setThemePref,
          controlledFonts: fonts,
          onFontsChange: setFonts,
          // GLS-FR-27: the rehearsal needs a real project to address. Null
          // while none is open, which greys the control out rather than sending
          // a notification nothing could route.
          rehearsalTarget:
            context && context.projectKey && context.contentRoot
              ? {
                  projectKey: context.projectKey,
                  worktree: context.contentRoot,
                }
              : null,
        }}
        project={
          projectOpen
            ? {
                // SET-FR-20: the Draft template section reads again whenever
                // this changes. It cannot change while the window is open
                // (SWN-FR-02), so in practice this settles once and the section
                // reads once.
                contentRoot: context?.contentRoot ?? "",
                lineEndings,
                agentEditorRequest: projectAgentEditorRequest,
                onAgentEditorRequestHandled: handleAgentEditorRequest,
                onSelectLineEndings: selectLineEndings,
                onOpenGithubTokenPicker: (currentTokenId, onConfirmed) =>
                  setPicker({ currentTokenId, onConfirmed }),
              }
            : undefined
        }
      />
      {picker && (
        <GithubTokenPicker
          projectName={context?.projectName ?? ""}
          // GHA-FR-20: presented within this window, so the note under the list
          // does not send the author to the window they are already in.
          inProjectSettings
          currentTokenId={picker.currentTokenId}
          onCancel={() => setPicker(null)}
          onConfirm={() => {
            const settled = picker;
            setPicker(null);
            settled.onConfirmed();
          }}
        />
      )}
      {/* EDT-FR-TOVY: the diagram editor overlay of the Draft template covers
          this window. SNV-FR-56 does not apply here. */}
      <DiagramEditorHost />
    </div>
  );
}

export default SettingsWindowApp;

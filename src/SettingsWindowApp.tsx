import "./styles/colors_and_type.css";
import "./styles/components.css";
import "./styles/kit.css";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import * as api from "./api";
import { GlobalSettings } from "./components/GlobalSettings";
import { Settings } from "./components/Settings";
import { GithubTokenPicker } from "./components/GithubTokenPicker";
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
  type SettingsWindowContext,
  type SettingsWindowKind,
} from "./settingsWindow";
import type { AgentEditorRequest } from "./components/Agents";

/**
 * The root of a settings **child window**
 * (`../specifications/ui/SWN-settings-windows.md`).
 *
 * Global settings and Project settings are native child windows rather than
 * tabs of the main viewport (SWN-FR-01), and each is a webview of its own. This
 * is what that webview renders: the window's one section-navigated body, and
 * the two things a window owes the rest of the application — the save sweep it
 * runs before it goes (SWN-FR-08) and the section it presents when asked to
 * (SWN-FR-13) or when a save fails (SWN-FR-11).
 *
 * Nothing of the main window's chrome is drawn here: no tab strip, no activity
 * bar, no status bar. The frame is the platform's own, and its size, position,
 * modality, and title all belong to the backend (`settings_window.rs`).
 */
export function SettingsWindowApp({
  kind,
  initialSection,
}: {
  kind: SettingsWindowKind;
  /** SWN-FR-13: the section the request that opened the window named. */
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
   * start (SET-FR-20).
   */
  const [context, setContext] = useState<SettingsWindowContext | null>(null);
  useEffect(() => {
    let cancelled = false;
    void api
      .getSettingsWindowContext()
      .then((ctx) => {
        if (!cancelled) setContext(ctx);
      })
      .catch((e) => {
        // The window still renders: Global settings is reachable with no
        // project at all (SWN-FR-15), and every section that needs one
        // disables itself rather than failing.
        if (!cancelled) setContext(null);
        logWarn(["frontend"], "settings window could not read its project", {
          window: kind,
          reason: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [kind]);

  /**
   * SWN-FR-13 / SWN-FR-11: the section to present, seeded from the request that
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
   * AGT-FR-06 / AGT-FR-07: the persona editor a route asked for.
   *
   * Derived from the section address rather than carried beside it, so the
   * chrome roster's two actions travel the same one-string route every other
   * sectioned request does — `agents:new` for a fresh persona, `agents:<id>`
   * for one that exists, and a bare `agents` for the section alone.
   */
  const [honouredEditor, setHonouredEditor] = useState<number | null>(null);
  const agentEditorRequest = useMemo<AgentEditorRequest | null>(() => {
    if (!sectionRequest) return null;
    // Honoured once: without this the section would reopen the editor every
    // time its agent list changed underneath it — which is every create, every
    // edit, and every delete the author makes while it is open.
    if (honouredEditor === sectionRequest.nonce) return null;
    const { section, item } = parseSectionAddress(sectionRequest.section);
    if (section !== "agents" || item === null) return null;
    return {
      agentId: item === NEW_AGENT_ITEM ? null : item,
      nonce: sectionRequest.nonce,
    };
    // Memoised on the request itself rather than rebuilt per render: the
    // section watches this object's identity, and a fresh one each render would
    // reopen the editor every time anything else in the window changed.
  }, [sectionRequest, honouredEditor]);

  useEffect(() => {
    let cancelled = false;
    const unlisten: Array<() => void> = [];
    const keep = (fn: () => void) => {
      if (cancelled) fn();
      else unlisten.push(fn);
    };
    // SWN-FR-06 / SWN-FR-13: a request for the window already open focuses it
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
   * Every close — the author's own, the first half of a switch, and the one a
   * quit performs first — reaches this window as one request, and this is the
   * whole of the answer: write every pending change in every section, then
   * report. No discard prompt exists anywhere in it (SWN-FR-10), because the
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
          window: kind,
          ok,
          reason: e instanceof Error ? e.message : String(e),
        });
      }
    },
    [kind],
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
            logInfo(["frontend"], "settings window saved its pending changes", {
              window: kind,
            });
            return answer(true, null);
          }
          // SWN-FR-11: the transition is cancelled, the window stays open, and
          // the failed section keeps the author's edits exactly as they were.
          logWarn(["frontend"], "settings window save failed; close cancelled", {
            window: kind,
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
  }, [answer, kind, presentSection]);

  return kind === "global" ? (
    <div className="app settings-window">
      <GlobalSettings
        sectionRequest={sectionRequest}
        agentEditorRequest={agentEditorRequest}
        onAgentEditorRequestHandled={() =>
          setHonouredEditor(sectionRequest?.nonce ?? null)
        }
        controlledTheme={themePref}
        onThemeChange={setThemePref}
        controlledFonts={fonts}
        onFontsChange={setFonts}
        // GLS-FR-27: the rehearsal needs a real project to address. Null while
        // none is open, which greys the control out rather than sending a
        // notification nothing could route.
        rehearsalTarget={
          context && context.projectKey && context.contentRoot
            ? {
                projectKey: context.projectKey,
                worktree: context.contentRoot,
              }
            : null
        }
      />
    </div>
  ) : (
    <ProjectSettingsWindow
      sectionRequest={sectionRequest}
      context={context}
    />
  );
}

/**
 * The Project settings window's body.
 *
 * Split out because it owns two things Global settings has no use for: the
 * project's line-ending convention, which it edits jointly with the main
 * window's status bar (SET-FR-11), and the GitHub token picker, which
 * GHA-FR-20 requires be presented **within this window** rather than over the
 * main one — that window being modal to it and unable to show anything.
 */
function ProjectSettingsWindow({
  sectionRequest,
  context,
}: {
  sectionRequest: SectionRequest | null;
  context: SettingsWindowContext | null;
}) {
  const { lineEndings, selectLineEndings } = useLineEndings(
    context?.contentRoot ?? "",
  );

  /**
   * GHA-FR-20 / GHA-FR-21: the picker, opened from the Project section with
   * nothing waiting on it. One instance for the window, exactly as the main
   * window holds one for itself, so it cannot coexist with a second.
   */
  const [picker, setPicker] = useState<{
    currentTokenId: string | null;
    onConfirmed: () => void;
  } | null>(null);

  return (
    <div className="app settings-window">
      <Settings
        sectionRequest={sectionRequest}
        // SET-FR-20: the Draft template section reads again whenever this
        // changes. It cannot change while the window is open (SWN-FR-02), so in
        // practice this settles once and the section reads once.
        contentRoot={context?.contentRoot ?? ""}
        lineEndings={lineEndings}
        onSelectLineEndings={selectLineEndings}
        onOpenGithubTokenPicker={(currentTokenId, onConfirmed) =>
          setPicker({ currentTokenId, onConfirmed })
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
    </div>
  );
}

export default SettingsWindowApp;

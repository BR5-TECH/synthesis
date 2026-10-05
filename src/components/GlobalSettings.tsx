import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import {
  loadAppPreferences,
  patchAppPreferences,
} from "../state/appPreferences";

import {
  registerSettingsSection,
  type SectionRequest,
} from "../state/settingsSweep";
import { parseSectionAddress } from "../settingsWindow";
import { logWarn } from "../logging";
import { SettingsSectionNav } from "./SettingsSectionNav";
import { Icon } from "./icons";
import {
  NotificationSettings,
  type RehearsalTarget,
} from "./NotificationSettings";
import { NavigationSettings } from "./NavigationSettings";
import { DockerSettings } from "./DockerSettings";
import { RemoteConnectivitySettings } from "./RemoteConnectivitySettings";
import { FontRoleControls } from "./FontRoleControls";
import { GithubTokens } from "./GithubTokens";
import {
  AgenticAiIntegrations,
  AiApiIntegrations,
} from "./AiIntegrations";
import { GlobalAgents, type AgentEditorRequest } from "./Agents";
import { formatRelative } from "./ProjectPicker";
import type {
  FontFamily,
  FontSettings,
  InstalledAdapter,
  InstalledPlugin,
  RecentProject,
  ThemePreference,
} from "../types";

// Implements specifications/ui/GLS-global-settings.md (GLS-FR-01..13). Backend
// is specifications/core/GSS-global-settings-storage.md; every operation name
// below matches that spec's contract surface byte-for-byte.

type SectionKey =
  | "appearance"
  | "navigation"
  | "notifications"
  | "recents"
  | "plugins"
  | "adapters"
  | "github"
  | "ai-api"
  | "ai-agentic"
  | "agents"
  | "docker"
  | "remote";

const SECTIONS: [SectionKey, string][] = [
  ["appearance", "Appearance"],
  // GLS-FR-03 / GLS-FR-28: the Navigation section, directly after Appearance
  // because the two answer the same kind of question — how the application
  // behaves for this author on this machine — where every section below is about
  // what it is configured to reach. Like Appearance, everything in it applies
  // immediately and it holds no dirty state.
  ["navigation", "Navigation"],
  // GLS-FR-03 / GLS-FR-25: the Notifications section, placed directly after
  // Appearance because the two are the same kind of question — how the
  // application presents itself to this author on this machine — where every
  // section below is about what the application is configured to reach.
  ["notifications", "Notifications"],
  ["recents", "Recent projects"],
  ["plugins", "Installed plugins"],
  ["adapters", "Installed adapters"],
  // GLS-FR-03 / GLS-FR-15: the GitHub section. Its content is owned by
  // `GHA-github-authentication.md`; this tab places it and states that it sits
  // outside the per-section save model.
  ["github", "GitHub"],
  // GLS-FR-03 / GLS-FR-16: the two AI sections, one per level of
  // `AII-ai-integrations.md`. They are two sections rather than one because the
  // levels present the same shape — tabs, a status line, a model selector, an
  // activation control — and stacked in a single section they read as one
  // continuous list of fields whose second half happens to be about agents.
  // Like GitHub, both sit outside the per-section save model. Both are distinct
  // from Installed adapters above: an adapter transforms Synthesis artifacts
  // into an external agent's workspace layout, while an AI integration is
  // something this application calls or drives itself. The AI API section's
  // endpoints and keys serve every call the application makes for itself, while
  // the model and the reasoning it holds serve the graduation loop alone — an
  // agent runs on what its own description in the Agents section carries
  // (GLS-FR-16, GLS-FR-24, `AII-ai-integrations.md` AII-FR-54).
  ["ai-api", "AI API"],
  ["ai-agentic", "Agentic AI"],
  // GLS-FR-03 / GLS-FR-24: the Agents section. It sits after the two AI
  // sections because a persona *names* one of the providers configured there —
  // describing an agent before verifying a provider is describing one that
  // could not be spoken to (AGT-FR-14). An agent takes only that provider's
  // endpoint and key from the AI API section, never that section's own model or
  // reasoning selection: this is the only place the model and the reasoning an
  // agent runs on are chosen. Like GitHub and both AI sections above, every
  // action in it applies immediately and it holds no dirty state.
  ["agents", "Agents"],
  // GLS-FR-03 / GLS-FR-29: the Docker section, last because it is about the
  // machine's own plumbing rather than about anything the author writes with —
  // and it is what the two sections above it eventually run inside. Like every
  // section from GitHub down, everything in it applies immediately and it holds
  // no dirty state.
  ["docker", "Docker"],
  // GLS-FR-03 / GLS-FR-KVNP: the Remote connectivity section, last because it
  // is about how this machine is reached from outside rather than about
  // anything the author writes with. Like every section from GitHub down,
  // everything in it applies immediately and it holds no dirty state. It
  // presents no field for a token or a key (GLS-FR-XDUJ).
  ["remote", "Remote connectivity"],
];

// Shared with the shell top-chrome theme selector (SNV-FR-15) so the two
// entry points to the theme preference present the same three choices and
// cannot drift.
export const THEME_CHOICES: [ThemePreference, string][] = [
  ["light", "Light"],
  ["dark", "Dark"],
  ["system", "System"],
];

/// Resolve a theme preference to the concrete appearance applied to the root.
/// "system" follows the OS scheme (OVW-FR-10 owns live tracking; here we only
/// resolve the current value for immediate application per GLS-FR-05).
export function resolveTheme(pref: ThemePreference): "light" | "dark" {
  if (pref === "system") {
    if (typeof window !== "undefined" && typeof window.matchMedia === "function") {
      return window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
    }
    // No detectable OS preference (e.g. matchMedia unavailable): fall back to
    // light, the conventional default for an undetermined scheme.
    return "light";
  }
  return pref;
}

function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "operation failed";
}

interface GlobalSettingsProps {
  /**
   * SWN-FR-06 / SWN-FR-11 / SWN-FR-13: present a named section — the one a
   * request routed to, or the one whose save failed. Ignored when it names a
   * section this window does not have.
   */
  sectionRequest?: SectionRequest | null;
  /// Report the selected theme *preference* so the app applies it to the root
  /// (GLS-FR-05). The app owns app-wide application and OS-scheme tracking
  /// (OVW-FR-09 / OVW-FR-10), so this surface reports the preference rather than
  /// a resolved value — keeping "system" intact upstream.
  onThemeChange?: (pref: ThemePreference) => void;
  /// The shared user-global theme preference owned by the app (SNV-FR-15). When
  /// the preference is changed elsewhere — the shell top-chrome selector — this
  /// section must reflect it so the two controls never disagree. Undefined when
  /// the section is rendered standalone (it then relies solely on its own load).
  controlledTheme?: ThemePreference;
  /// GLS-FR-17 / OVW-FR-14: the three typographic roles the app root is set
  /// from. Reported up on change (`onFontsChange`) so the root applies the
  /// selection app-wide immediately (GLS-FR-20), and read back down so this
  /// section and the application can never disagree about what is applied —
  /// the same two-way arrangement the theme uses.
  controlledFonts?: FontSettings;
  onFontsChange?: (fonts: FontSettings | undefined) => void;
  /// AGT-FR-06 / AGT-FR-07: the chrome roster asking this tab to open an editor
  /// on its Agents section — an agent's when a row was activated, a fresh one
  /// when **Add an agent** was. `null` is every other way this tab is reached.
  agentEditorRequest?: AgentEditorRequest | null;
  /// GLS-FR-27: the project the Notifications section's rehearsal addresses, so
  /// the notification it sends lands on a tab that actually resolves
  /// (`NTF-notifications.md` NTF-FR-25). Undefined when the tab is rendered
  /// standalone, which disables the rehearsal rather than sending one nothing
  /// could route.
  rehearsalTarget?: RehearsalTarget | null;
  /// Called once the request has been honoured, so revisiting the section does
  /// not reopen the editor.
  onAgentEditorRequestHandled?: () => void;
}

export function GlobalSettings({
  sectionRequest,
  onThemeChange,
  controlledTheme,
  controlledFonts,
  onFontsChange,
  agentEditorRequest,
  onAgentEditorRequestHandled,
  rehearsalTarget,
}: GlobalSettingsProps) {
  const [section, setSection] = useState<SectionKey>("appearance");

  // SWN-FR-13: a route the window was opened or focused on, and SWN-FR-11's
  // presentation of the section whose save failed. Keyed on the nonce rather
  // than on the section, so the same section asked for twice presents twice.
  useEffect(() => {
    if (!sectionRequest) return;
    // The address may name something within the section (`agents:<id>`), which
    // the window turns into an editor request; the section it selects is the
    // part before the colon either way.
    const { section: wanted } = parseSectionAddress(sectionRequest.section);
    const known = SECTIONS.find(([k]) => k === wanted);
    if (known) setSection(known[0]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sectionRequest?.nonce, sectionRequest?.section]);

  // AGT-FR-06: a requested editor implies the section it lives in, so the tab
  // shows that section rather than opening a modal over Appearance. Runs on
  // every request rather than only at mount, because the roster addresses this
  // tab whether or not it was already open.
  useEffect(() => {
    if (agentEditorRequest) setSection("agents");
  }, [agentEditorRequest]);

  // Appearance (GLS-FR-04 / GLS-FR-05). `saved` is the last successfully
  // persisted value; the section is dirty when the selection diverges from it
  // (i.e. a selection was applied to the root but not yet persisted).
  const [theme, setTheme] = useState<ThemePreference | null>(null);
  const [savedTheme, setSavedTheme] = useState<ThemePreference | null>(null);
  const [appearanceError, setAppearanceError] = useState("");

  // Typography (GLS-FR-17..GLS-FR-23). `fonts` mirrors what the app root has
  // applied: the section's own selections go up through `onFontsChange` first
  // and come back down as `controlledFonts`, so there is one applied value
  // rather than two that have to be kept in step. Local state is the fallback
  // for a standalone mount, where nothing above is holding the value.
  const [localFonts, setLocalFonts] = useState<FontSettings | undefined>(
    undefined,
  );
  const fonts = controlledFonts ?? localFonts;
  // GLS-FR-18: the families the family controls offer. `null` while the query
  // is in flight; an empty list is a real answer (FNT-FR-07) and the controls
  // still offer the built-in faces.
  const [families, setFamilies] = useState<FontFamily[] | null>(null);

  // Recent projects (GLS-FR-06 / GLS-FR-07).
  const [recents, setRecents] = useState<RecentProject[] | null>(null);
  const [recentsError, setRecentsError] = useState("");

  // Installed plugins (GLS-FR-08).
  const [plugins, setPlugins] = useState<InstalledPlugin[] | null>(null);
  const [pluginsError, setPluginsError] = useState("");

  // Installed adapters (GLS-FR-09).
  const [adapters, setAdapters] = useState<InstalledAdapter[] | null>(null);
  const [adaptersError, setAdaptersError] = useState("");

  // GLS-FR-10 / GLS-FR-13: the window's one pending change. In this build only
  // Appearance can strand one — a selection applied to the root whose
  // persistence failed; every other section persists its mutations immediately
  // and contributes nothing to the save-before-close sweep (SWN-FR-08).
  const appearanceDirty = theme !== null && theme !== savedTheme;

  /**
   * SWN-FR-08 / SWN-FR-09: publish Appearance's pending change to the window's
   * save-before-close sweep.
   *
   * Read through refs rather than captured, because the registration is made
   * once and the sweep can arrive at any moment: a captured `theme` would offer
   * the sweep whatever the section held when it last registered. The section
   * need not be on screen for this to be spent — that is the whole point of
   * SWN-FR-08 — so what it publishes has to be current rather than rendered.
   */
  const themeRef = useRef(theme);
  themeRef.current = theme;
  const appearanceDirtyRef = useRef(appearanceDirty);
  appearanceDirtyRef.current = appearanceDirty;
  useEffect(
    () =>
      registerSettingsSection({
        section: "appearance",
        pending: () => appearanceDirtyRef.current,
        save: async () => {
          const pref = themeRef.current;
          if (pref === null) return true;
          try {
            await patchAppPreferences({ theme: pref });
            setSavedTheme(pref);
            setAppearanceError("");
            return true;
          } catch (e) {
            // SWN-FR-11: the section keeps the author's selection exactly as it
            // was and presents its own error and its own retry — the retry
            // being the selector itself, which persists on every selection.
            setAppearanceError(errorMessage(e));
            logWarn(["frontend"], "Global settings appearance save failed", {
              // The theme is a fixed vocabulary of three words, never author
              // content, so it is what explains the record.
              theme: pref,
            });
            return false;
          }
        },
      }),
    [],
  );

  // GLS-FR-04: initialise the theme selector from "load app preferences" on
  // mount, and apply the persisted value to the root so the selector and the
  // app agree from the first paint.
  //
  // GLS-FR-17: the same read initialises all nine typography controls, in the
  // one round-trip the record is designed to be readable in (GSS-FR-04) rather
  // than a second call for the fonts.
  useEffect(() => {
    loadAppPreferences()
      .then((prefs) => {
        const pref = prefs?.theme ?? "system";
        setTheme(pref);
        setSavedTheme(pref);
        onThemeChange?.(pref);
        setLocalFonts(prefs?.fonts);
        onFontsChange?.(prefs?.fonts);
      })
      .catch((e) => {
        setTheme("system");
        setSavedTheme("system");
        setAppearanceError(errorMessage(e));
      });
    // onThemeChange/onFontsChange intentionally excluded: mount-only
    // initialisation.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // GLS-FR-18 / FNT-FR-01: the installed families, read once for the lifetime
  // of the surface — the enumeration reflects the machine at the moment of the
  // call (FNT-FR-06) and re-querying it per interaction would buy nothing.
  // FNT-FR-07 promises the command never fails; the catch is here because a
  // bridge that is not there at all (a non-Tauri mount) still rejects, and an
  // empty list is what the controls handle anyway.
  useEffect(() => {
    api.listSystemFonts()
      .then((list) => setFamilies(list ?? []))
      .catch(() => setFamilies([]));
  }, []);

  // SNV-FR-15: reflect a preference changed from the shell top-chrome selector.
  // Such a change is already persisted by the app, so adopt it as both the
  // displayed and the saved baseline (keeping this section clean). The guard
  // `controlledTheme === theme` skips this section's *own* selections — those
  // set `theme` locally first, so a failed self-save still strands correctly
  // (theme diverges from savedTheme) per GLS-FR-13.
  useEffect(() => {
    if (controlledTheme == null || controlledTheme === theme) return;
    setTheme(controlledTheme);
    setSavedTheme(controlledTheme);
    // theme/savedTheme are intentionally not deps: this reacts to external
    // (controlledTheme) changes only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [controlledTheme]);

  const loadRecents = () => {
    api.listRecentProjects()
      .then((list) => {
        setRecents(list);
        setRecentsError("");
      })
      .catch((e) => {
        setRecents([]);
        setRecentsError(errorMessage(e));
      });
  };
  const loadPlugins = () => {
    api.listInstalledPlugins()
      .then((list) => {
        setPlugins(list);
        setPluginsError("");
      })
      .catch((e) => {
        setPlugins([]);
        setPluginsError(errorMessage(e));
      });
  };
  const loadAdapters = () => {
    api.listAgentAdapters()
      .then((list) => {
        setAdapters(list);
        setAdaptersError("");
      })
      .catch((e) => {
        setAdapters([]);
        setAdaptersError(errorMessage(e));
      });
  };

  useEffect(() => {
    loadRecents();
    loadPlugins();
    loadAdapters();
  }, []);

  // GLS-FR-05: selecting a theme applies it to the root immediately and
  // persists it. If persistence fails the section stays dirty (the value is
  // applied but unsaved) and an inline error is shown.
  //
  // GLS-FR-14 / GSS-FR-20: this section edits the theme and nothing else, but
  // `save_app_preferences` writes the record whole — and that record also
  // carries the main window's full-screen state, which the shell owns
  // (SNV-FR-38) and this tab offers no control for. Patching carries it through
  // unchanged; a bare `{ theme }` write would silently clear it.
  const onSelectTheme = async (pref: ThemePreference) => {
    setTheme(pref);
    onThemeChange?.(pref);
    setAppearanceError("");
    try {
      await patchAppPreferences({ theme: pref });
      setSavedTheme(pref);
    } catch (e) {
      setAppearanceError(errorMessage(e));
    }
  };

  // GLS-FR-20: a change to any of the nine controls applies to the whole
  // application immediately and persists at once. The section therefore holds
  // no dirty state and never contributes to GLS-FR-13's confirmation — exactly
  // as the theme selector does not.
  //
  // GLS-FR-14 / GSS-FR-20: patched rather than replaced, because
  // `save_app_preferences` writes the record whole and this record also carries
  // the theme, the window's full-screen state, the search query mode, both diff
  // modes, and the Changes panel's action, none of which this section edits.
  //
  // A failed write rolls the applied value back rather than leaving the app
  // typeset in a choice that will not survive relaunch.
  const onChangeFonts = async (next: FontSettings) => {
    const previous = fonts;
    setLocalFonts(next);
    onFontsChange?.(next);
    setAppearanceError("");
    try {
      await patchAppPreferences({ fonts: next });
    } catch (e) {
      setLocalFonts(previous);
      onFontsChange?.(previous);
      setAppearanceError(errorMessage(e));
    }
  };

  const onRemoveRecent = async (path: string) => {
    try {
      await api.removeRecentProject(path);
      loadRecents();
    } catch (e) {
      setRecentsError(errorMessage(e));
    }
  };
  const onClearRecents = async () => {
    try {
      await api.clearRecentProjects();
      loadRecents();
    } catch (e) {
      setRecentsError(errorMessage(e));
    }
  };
  const onTogglePin = async (r: RecentProject) => {
    try {
      await (r.pinned ? api.unpinRecentProject(r.path) : api.pinRecentProject(r.path));
      loadRecents();
    } catch (e) {
      setRecentsError(errorMessage(e));
    }
  };

  const onInstallPlugin = async () => {
    const source = window.prompt("Plugin source (path or git URL)")?.trim();
    if (!source) return;
    try {
      await api.installPlugin(source);
      loadPlugins();
    } catch (e) {
      setPluginsError(errorMessage(e));
    }
  };
  const onUninstallPlugin = async (id: string) => {
    try {
      await api.uninstallPlugin(id);
      loadPlugins();
    } catch (e) {
      setPluginsError(errorMessage(e));
    }
  };
  const onInstallAdapter = async () => {
    const source = window.prompt("Adapter source (path or git URL)")?.trim();
    if (!source) return;
    try {
      await api.installAdapter(source);
      loadAdapters();
    } catch (e) {
      setAdaptersError(errorMessage(e));
    }
  };

  const current = SECTIONS.find((s) => s[0] === section)!;

  return (
    <div
      style={{ display: "grid", gridTemplateColumns: "200px 1fr", height: "100%" }}
      data-testid="global-settings"
    >
      <SettingsSectionNav
        sections={SECTIONS}
        selected={section}
        onSelect={setSection}
        label="Global settings sections"
      />

      <div style={{ padding: "28px 36px", maxWidth: 760, overflow: "auto" }}>
        <h2 className="t-h2" style={{ margin: "0 0 4px" }}>
          {current[1]}
        </h2>

        {section === "appearance" && (
          <div>
            <p className="t-p" style={{ marginBottom: 20 }}>
              Choose how Synthesis looks. Applies immediately across the app.
            </p>
            <div
              role="radiogroup"
              aria-label="Theme"
              style={{ display: "flex", gap: 8 }}
            >
              {THEME_CHOICES.map(([value, label]) => (
                <button
                  key={value}
                  role="radio"
                  aria-checked={theme === value}
                  className="btn btn--default btn--sm"
                  data-active={theme === value}
                  style={{
                    background:
                      theme === value ? "var(--bg-active)" : undefined,
                    borderColor:
                      theme === value ? "var(--accent)" : undefined,
                  }}
                  onClick={() => onSelectTheme(value)}
                >
                  {value === "light" && <Icon.Sun size={13} />}
                  {value === "dark" && <Icon.Moon size={13} />}
                  {value === "system" && <Icon.Settings size={13} />}
                  {label}
                </button>
              ))}
            </div>
            {appearanceDirty && (
              <p className="t-ui-xs t-muted" style={{ marginTop: 10 }}>
                Unsaved theme change.
              </p>
            )}

            {/* GLS-FR-17: the three typographic roles. Placed in Appearance
                rather than a section of their own — the theme and the fonts are
                one question about how the app looks, asked in one place. */}
            <h3 className="t-eyebrow" style={{ margin: "28px 0 10px" }}>
              Typography
            </h3>
            <p className="t-p" style={{ marginBottom: 16 }}>
              Every piece of text belongs to one of three roles. Changes apply
              across the app as you make them.
            </p>
            <FontRoleControls
              fonts={fonts}
              families={families}
              onChange={(next) => void onChangeFonts(next)}
            />

            {appearanceError && (
              <span className="picker-error" style={{ display: "block", marginTop: 8 }}>
                ✗ {appearanceError}
              </span>
            )}
          </div>
        )}

        {section === "navigation" && <NavigationSettings />}

        {section === "notifications" && (
          <NotificationSettings rehearsalTarget={rehearsalTarget ?? null} />
        )}

        {section === "recents" && (
          <div>
            <p className="t-p" style={{ marginBottom: 20 }}>
              Manage the projects shown in the picker. Removing or clearing
              never deletes a project on disk.
            </p>
            {recents === null && <div className="t-muted">Loading…</div>}
            {recents !== null && recents.length === 0 && (
              <div
                className="card"
                style={{ padding: "32px 20px", textAlign: "center", color: "var(--fg-3)" }}
              >
                No recent projects yet.
              </div>
            )}
            {recents !== null && recents.length > 0 && (
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {recents.map((r) => (
                  <div
                    key={`${r.name}:${r.path}`}
                    className="card"
                    data-testid="recent-row"
                    style={{ display: "flex", alignItems: "center", gap: 12, padding: "10px 14px" }}
                  >
                    <Icon.Folder size={14} className="icon" />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>
                        {r.name}
                        {r.pinned && (
                          <span className="badge" style={{ marginLeft: 6 }}>
                            pinned
                          </span>
                        )}
                        {r.missing && (
                          <span className="badge badge--warn" style={{ marginLeft: 6 }}>
                            missing
                          </span>
                        )}
                      </div>
                      <div className="t-ui-sm t-muted">{r.path}</div>
                    </div>
                    <span className="t-meta">{formatRelative(r.lastOpenedAt)}</span>
                    <button
                      className="btn btn--ghost btn--sm"
                      onClick={() => onTogglePin(r)}
                    >
                      {r.pinned ? "Unpin" : "Pin"}
                    </button>
                    <button
                      className="btn btn--ghost btn--icon btn--sm"
                      title="Remove from list"
                      aria-label={`Remove ${r.name}`}
                      onClick={() => onRemoveRecent(r.path)}
                    >
                      <Icon.X size={12} />
                    </button>
                  </div>
                ))}
                <button
                  className="btn btn--default btn--sm"
                  style={{ alignSelf: "flex-start", marginTop: 8 }}
                  onClick={onClearRecents}
                >
                  Clear all
                </button>
              </div>
            )}
            {recentsError && (
              <span className="picker-error" style={{ display: "block", marginTop: 8 }}>
                ✗ {recentsError}
              </span>
            )}
          </div>
        )}

        {section === "plugins" && (
          <div>
            <p className="t-p" style={{ marginBottom: 20 }}>
              Plugins installed for this machine. Enablement per project lives in
              Project settings.
            </p>
            {plugins === null && <div className="t-muted">Loading…</div>}
            {plugins !== null && plugins.length === 0 && (
              <div
                className="card"
                style={{ padding: "32px 20px", textAlign: "center", color: "var(--fg-3)" }}
              >
                No plugins installed.
              </div>
            )}
            {plugins !== null && plugins.length > 0 && (
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {plugins.map((p) => (
                  <div
                    key={p.id}
                    className="card"
                    data-testid="plugin-row"
                    style={{ display: "flex", alignItems: "center", gap: 12, padding: "10px 14px" }}
                  >
                    <div style={{ flex: 1 }}>
                      <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>
                        {p.name}
                      </div>
                      <div className="t-ui-sm t-muted">{p.source}</div>
                    </div>
                    <button
                      className="btn btn--ghost btn--sm"
                      aria-label={`Uninstall ${p.name}`}
                      onClick={() => onUninstallPlugin(p.id)}
                    >
                      Uninstall
                    </button>
                  </div>
                ))}
              </div>
            )}
            <button
              className="btn btn--default btn--sm"
              style={{ alignSelf: "flex-start", marginTop: 12 }}
              onClick={onInstallPlugin}
            >
              <Icon.Plus size={12} /> Install plugin…
            </button>
            {pluginsError && (
              <span className="picker-error" style={{ display: "block", marginTop: 8 }}>
                ✗ {pluginsError}
              </span>
            )}
          </div>
        )}

        {section === "adapters" && (
          <div>
            <p className="t-p" style={{ marginBottom: 20 }}>
              Agent adapters installed for this machine. Per-project
              configuration lives in Project settings.
            </p>
            {adapters === null && <div className="t-muted">Loading…</div>}
            {adapters !== null && adapters.length === 0 && (
              <div
                className="card"
                style={{ padding: "32px 20px", textAlign: "center", color: "var(--fg-3)" }}
              >
                No adapters installed.
              </div>
            )}
            {adapters !== null && adapters.length > 0 && (
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {adapters.map((a) => (
                  <div
                    key={a.id}
                    className="card"
                    data-testid="adapter-row"
                    style={{ display: "flex", alignItems: "center", gap: 12, padding: "10px 14px" }}
                  >
                    <div style={{ flex: 1 }}>
                      <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>
                        {a.name}
                      </div>
                      <div className="t-ui-sm t-muted">{a.source}</div>
                    </div>
                  </div>
                ))}
              </div>
            )}
            <button
              className="btn btn--default btn--sm"
              style={{ alignSelf: "flex-start", marginTop: 12 }}
              onClick={onInstallAdapter}
            >
              <Icon.Plus size={12} /> Install adapter…
            </button>
            {adaptersError && (
              <span className="picker-error" style={{ display: "block", marginTop: 8 }}>
                ✗ {adaptersError}
              </span>
            )}
          </div>
        )}

        {/* GLS-FR-15: every action in this section applies immediately, so it
            reports no dirty state and never contributes to the window's
            save-before-close sweep (GLS-FR-13, SWN-FR-08). */}
        {section === "github" && <GithubTokens />}

        {/* GLS-FR-16: every action in either section applies immediately, so
            neither reports a dirty state nor contributes to the window's
            save-before-close sweep (GLS-FR-13). Only the selected one is
            mounted, which is what keeps a level's requests, state, and errors
            wholly its own (AII-FR-02). */}
        {section === "ai-api" && <AiApiIntegrations />}
        {section === "ai-agentic" && <AgenticAiIntegrations />}

        {/* GLS-FR-24 / AGT-FR-21: every action here applies immediately through
            its own operation, so the section never contributes to the window's
            save-before-close sweep (GLS-FR-13, SWN-FR-08). */}
        {section === "docker" && <DockerSettings />}
        {/* GLS-FR-RBLM: every change applies immediately through its own
            operation, so the section never contributes to the window's
            save-before-close sweep (GLS-FR-13, SWN-FR-08). */}
        {section === "remote" && <RemoteConnectivitySettings />}
        {section === "agents" && (
          <GlobalAgents
            editorRequest={agentEditorRequest}
            onEditorRequestHandled={onAgentEditorRequestHandled}
          />
        )}
      </div>
    </div>
  );
}

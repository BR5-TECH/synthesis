import { useEffect, useState } from "react";
import * as api from "../api";
import { Icon } from "./icons";
import { TokenIdentityLine, tokenErrorMessage } from "./GithubTokens";
import { ProjectAiIntegrations } from "./AiIntegrations";
import { ProjectAgents } from "./Agents";
import { DraftTemplateSection } from "./DraftTemplateSection";
import { GithubPollingSection } from "./GithubPollingSection";
import { GithubPublicationSettingsGroup } from "./GithubPublicationSettings";
import { GraduationSettingsSection } from "./GraduationSettingsSection";
import { ProjectDockerImages } from "./ProjectDockerImages";
import { GraduationSettings } from "./GraduationSettings";
import { SettingsSectionNav } from "./SettingsSectionNav";
import { LINE_ENDING_CHOICES } from "../hooks/useLineEndings";
import type { SectionRequest } from "../state/settingsSweep";
import { useGraduationConcurrencySetting } from "../state/graduationConcurrency";
import { parseSectionAddress } from "../settingsWindow";
import type { GithubTokenBinding, GithubTokenRecord, LineEndings } from "../types";

type SectionKey =
  | "project"
  | "template"
  | "bindings"
  | "github-polling"
  | "adapters"
  | "mcp"
  | "plugins"
  | "agents"
  | "docker"
  | "graduation";

/**
 * A section's nav label and the sentence under its heading. The sentence is
 * written per section rather than derived from the label: "Project" would
 * derive the tautology "Manage project for this project", and lowercasing a
 * label to fit a template destroys an acronym ("Manage mcp servers…").
 */
const sections: [SectionKey, string, string][] = [
  ["project", "Project", "How this project writes, and who it works through."],
  // SET-FR-03 / SET-FR-16: the Draft template section, second in the tab's
  // order. It holds the project's optional Markdown draft template and nothing
  // else — the starting content every draft here is born holding (DRS-FR-39).
  [
    "template",
    "Draft template",
    "The text every new draft in this project starts with.",
  ],
  [
    "bindings",
    "Target repo bindings",
    "Where this project's artifacts are published.",
  ],
  // SET-FR-03 / SET-FR-TTTB: the GitHub Project settings section. Its polling
  // controls and its publication group persist at once, outside the section
  // dirty state (SET-FR-VLQJ, SET-FR-MQUE).
  [
    "github-polling",
    "GitHub Project settings",
    "Which GitHub Project the Git panel reads ready tasks from, how often, and how drafts are published as issues.",
  ],
  ["adapters", "Agent adapters", "The agent backends available to this project."],
  ["mcp", "MCP servers", "The MCP servers this project exposes artifacts to."],
  ["plugins", "Plugins", "Manage plugins for this project."],
  // SET-FR-15: the Agents section. It enrols the personas described in Global
  // settings (AGT-FR-22) and describes none of its own, which is why it names
  // where they come from rather than offering an editor.
  ["agents", "Agents", "Who this project can talk to."],
  // SET-FR-03 / SET-FR-21: the Docker section. It settles which container image
  // each agentic CLI vendor's work runs in for this project, and nothing about
  // how this machine reaches Docker — that belongs to Global settings
  // (GLS-FR-29). Like the Agents section, everything in it applies at once.
  [
    "docker",
    "Docker",
    "The container image each agent's work runs in here.",
  ],
  // SET-FR-03 / SET-FR-DHFS / SET-FR-QKKQ: the Graduation section. Its time
  // limit row persists at once, so it holds no dirty state (SET-FR-YMRV); its
  // concurrency limit is a pending change the window writes with its other
  // pending changes (SET-FR-HQKK).
  [
    "graduation",
    "Graduation",
    "How many graduation runs this project works at the same time, and how long one agent turn may run.",
  ],
];

const plugins: [string, string, boolean][] = [
  ["markdown-toolbar", "Built-in Tiptap markdown toolbar", true],
  ["git-pr", "Git pull-request integration", true],
  ["mcp-bridge", "MCP bridge — expose artifacts to remote agents", false],
  ["scenario-coverage", "Spec-to-scenario coverage canvas (preview)", true],
];

/**
 * SET-FR-10 / SET-FR-11: the line-ending convention is a *shared* project
 * preference, not this tab's own state — the status bar edits the very same
 * value (STB-FR-16). Both are handed the value and the setter from one owner
 * (`useLineEndings`), which is what makes each reflect a change made from the
 * other without a reload.
 */
export interface SettingsProps {
  /**
   * SWN-FR-06 / SWN-FR-11 / SWN-FR-13: present a named section — the one a
   * request routed to, or the one whose save failed. Ignored when it names a
   * section this window does not have.
   */
  sectionRequest?: SectionRequest | null;
  /**
   * SET-FR-20: the open project and its active worktree, as one key. The Draft
   * template section reads again whenever it changes, so it never shows — or
   * overwrites — a template that is not the active worktree's own.
   */
  contentRoot: string;
  lineEndings: LineEndings | null;
  onSelectLineEndings: (value: LineEndings) => void;
  /**
   * SET-FR-12: open the token picker with nothing waiting on it (GHA-FR-20).
   * The callback fires once a selection has been persisted, so this tab can
   * re-read the binding it just changed.
   */
  onOpenGithubTokenPicker?: (
    currentTokenId: string | null,
    onConfirmed: () => void,
  ) => void;
}

export function Settings({
  sectionRequest,
  contentRoot,
  lineEndings,
  onSelectLineEndings,
  onOpenGithubTokenPicker,
}: SettingsProps) {
  // SET-FR-03 / SWN-FR-13: the window opens on the first section it lists —
  // the one that settles how the project writes and who it works through —
  // rather than on whichever section happened to be built first.
  const [section, setSection] = useState<SectionKey>("project");
  const current = sections.find((s) => s[0] === section)!;

  // SET-FR-HQKK / SET-FR-IHQE: the Graduation limit lives here, with the window,
  // so a choice survives a visit to another section and the close sweep reaches
  // it whichever section is on screen.
  const graduation = useGraduationConcurrencySetting(contentRoot);

  // SWN-FR-13: a route the window was opened or focused on, and SWN-FR-11's
  // presentation of the section whose save failed. Keyed on the nonce rather
  // than on the section, so the same section asked for twice presents twice.
  useEffect(() => {
    if (!sectionRequest) return;
    const { section: wanted } = parseSectionAddress(sectionRequest.section);
    const known = sections.find(([k]) => k === wanted);
    if (known) setSection(known[0]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sectionRequest?.nonce, sectionRequest?.section]);

  // SET-FR-12: which GitHub token this project's authenticated operations use.
  // Read on mount; the picker is what changes it, and the change persists at
  // once — so this control sits outside the section's dirty state (SET-FR-08).
  const [binding, setBinding] = useState<GithubTokenBinding | null>(null);
  const [tokens, setTokens] = useState<GithubTokenRecord[]>([]);
  const [tokenError, setTokenError] = useState("");

  const loadBinding = () =>
    Promise.all([api.getProjectGithubTokenBinding(), api.listGithubTokens()])
      .then(([b, list]) => {
        setBinding(b ?? null);
        // Defensive: the section must not crash on a backend (or a test double)
        // that answers with nothing where a list was promised.
        setTokens(Array.isArray(list) ? list : []);
        setTokenError("");
      })
      .catch((e) => {
        setBinding(null);
        setTokenError(tokenErrorMessage(e));
      });

  useEffect(() => {
    void loadBinding();
    // Mount-only: the picker's confirm callback is what re-reads afterwards.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const boundToken = tokens.find((t) => t.id === binding?.tokenId) ?? null;

  return (
    <div style={{ display: "grid", gridTemplateColumns: "200px 1fr", height: "100%" }}>
      <SettingsSectionNav
        sections={sections}
        selected={section}
        onSelect={setSection}
        label="Project settings sections"
      />
      {/* SWN-FR-03: the window is fixed at 800 × 600, so the content column is
          a flex column rather than a block that grows — a section that fills
          the space available (the Draft template's editing surface) can then be
          sized to it rather than running past a fold nothing can be resized
          past. `minHeight: 0` is what lets it shrink inside the grid track. */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          minHeight: 0,
          padding: "28px 36px",
          maxWidth: 760,
          overflow: "auto",
        }}
      >
        <h2 className="t-h2" style={{ margin: "0 0 4px" }}>
          {current[1]}
        </h2>
        <p className="t-p" style={{ marginBottom: 24 }}>
          {current[2]}
        </p>

        {/* SET-FR-16 … SET-FR-20: the Draft template section. Keyed on the
            content root so a project or worktree change gives it a fresh mount
            rather than one holding the outgoing project's buffer. */}
        {section === "template" && (
          <DraftTemplateSection contentRoot={contentRoot} />
        )}

        {section === "plugins" && (
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {plugins.map(([id, label, on]) => (
              <div
                key={id}
                className="card"
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 12,
                  padding: "12px 14px",
                }}
              >
                <div style={{ flex: 1 }}>
                  <div
                    style={{
                      fontSize: "var(--fs-ui-md)",
                      fontWeight: 600,
                      color: "var(--fg-1)",
                    }}
                  >
                    {id}
                  </div>
                  <div className="t-ui-sm t-muted">{label}</div>
                </div>
                <button className="btn btn--ghost btn--sm">Configure</button>
                <div className="toggle" data-on={on}></div>
              </div>
            ))}
            <button
              className="btn btn--default btn--sm"
              style={{ alignSelf: "flex-start", marginTop: 8 }}
            >
              <Icon.Plus size={12} /> Install plugin…
            </button>
          </div>
        )}

        {/* SET-FR-10: exactly two mutually-exclusive choices, persisted the
            moment one is selected rather than on a section save — so this
            control sits outside the section's dirty state (SET-FR-08) and
            behaves identically to the status bar's (SET-FR-11). Every artifact
            written from then on uses it (PST-FR-23). */}
        {section === "project" && lineEndings && (
          <div
            className="card"
            style={{ padding: "14px 16px", marginBottom: 16 }}
          >
            <div
              style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}
            >
              Line endings
            </div>
            <div className="t-ui-sm t-muted" style={{ marginBottom: 10 }}>
              Every artifact this project writes uses this convention.
            </div>
            <div
              role="radiogroup"
              aria-label="Line endings"
              data-testid="settings-line-endings"
              style={{ display: "inline-flex", gap: 6 }}
            >
              {LINE_ENDING_CHOICES.map(([value, label]) => (
                <button
                  key={value}
                  role="radio"
                  aria-checked={lineEndings === value}
                  aria-label={label}
                  className="btn btn--ghost btn--sm"
                  data-active={lineEndings === value}
                  style={{
                    background:
                      lineEndings === value ? "var(--bg-active)" : undefined,
                    color: lineEndings === value ? "var(--accent)" : undefined,
                  }}
                  onClick={() => onSelectLineEndings(value)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        )}

        {/* SET-FR-12: which GitHub token this project authenticates with. The
            token is described, never rendered beyond its masked hint
            (GHA-FR-03), and changing it goes through the same picker modal the
            Git panel opens (GHA-FR-20). */}
        {section === "project" && (
          <div
            className="card"
            style={{ padding: "14px 16px", marginBottom: 16 }}
            data-testid="settings-github-token"
          >
            <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>
              GitHub token
            </div>
            <div className="t-ui-sm t-muted" style={{ marginBottom: 10 }}>
              Used when this project opens pull requests or pushes over HTTPS.
            </div>

            {binding === null && !tokenError && (
              <div className="t-muted t-ui-sm">Loading…</div>
            )}

            {binding?.resolution === "none_stored" && (
              <div className="t-ui-sm" data-testid="github-token-none">
                No token is stored. Add one in Global settings → GitHub.
              </div>
            )}

            {binding?.resolution === "selection_required" && (
              <div className="t-ui-sm" data-testid="github-token-unbound">
                No token chosen for this project yet.
              </div>
            )}

            {boundToken &&
              (binding?.resolution === "bound" ||
                binding?.resolution === "implicit") && (
                <div data-testid="github-token-bound">
                  <div style={{ fontSize: "var(--fs-ui-md)", color: "var(--fg-1)" }}>
                    {boundToken.label}
                  </div>
                  <TokenIdentityLine token={boundToken} />
                  {binding.resolution === "implicit" && (
                    <div className="t-ui-xs t-muted" style={{ marginTop: 4 }}>
                      Used automatically — it is the only token stored.
                    </div>
                  )}
                </div>
              )}

            {/* Nothing to choose between until at least one token exists. */}
            {binding && binding.resolution !== "none_stored" && (
              <button
                className="btn btn--ghost btn--sm btn--inline-start"
                style={{ marginTop: 10 }}
                onClick={() =>
                  onOpenGithubTokenPicker?.(binding.tokenId, () =>
                    void loadBinding(),
                  )
                }
              >
                Change…
              </button>
            )}

            {tokenError && (
              <span className="picker-error" style={{ display: "block", marginTop: 8 }}>
                ✗ {tokenError}
              </span>
            )}
          </div>
        )}

        {/* SET-FR-13: which AI CLI integration this project uses and how it
            resolved. The control's content and behaviour are owned by
            `AII-ai-integrations.md` (AII-FR-17..20); the integrations
            themselves are configured and verified in Global settings
            (GLS-FR-16), never here. It applies at once, so it sits outside the
            section's dirty state (SET-FR-08). */}
        {section === "project" && <ProjectAiIntegrations />}

        {/* SET-FR-15: enrolment applies at once through its own operation, so
            this section holds no dirty state and never contributes to the tab's
            save-before-close sweep (SET-FR-08, SWN-FR-08). */}
        {section === "agents" && <ProjectAgents />}

        {/* SET-FR-21 through SET-FR-27: the Docker section. Every field in it
            persists at once, so it holds no dirty state and takes no part in
            the tab's save-before-close sweep (SET-FR-08, SWN-FR-08). */}
        {section === "docker" && <ProjectDockerImages />}

        {/* SET-FR-QKKQ through SET-FR-IHQE: the concurrency limit. SET-FR-DHFS /
            SET-FR-YMRV: the time limit row persists at once, so it takes no
            part in the save-before-close sweep (SET-FR-08, SWN-FR-08). */}
        {section === "graduation" && (
          <>
            <GraduationSettings setting={graduation} />
            <GraduationSettingsSection />
          </>
        )}

        {/* SET-FR-TTTB through SET-FR-GXJU, SET-FR-BLQN through SET-FR-LJYT: the
            GitHub Project settings section. */}
        {section === "github-polling" && (
          <>
            <GithubPollingSection />
            <GithubPublicationSettingsGroup />
          </>
        )}

        {/* The placeholder for the sections that are still stubs. Listed as the
            ones it *excludes*, so a section built later has to be named here —
            the Draft template section is one of them (SET-FR-16), and the stub
            painted under its editor otherwise. */}
        {section !== "plugins" &&
          section !== "project" &&
          section !== "template" &&
          section !== "docker" &&
          section !== "graduation" &&
          section !== "github-polling" &&
          section !== "agents" && (
          <div
            className="card"
            style={{
              textAlign: "center",
              padding: "40px 20px",
              color: "var(--fg-3)",
            }}
          >
            <div style={{ fontSize: "var(--fs-ui-md)" }}>
              Section forms are stubbed in this UI kit.
            </div>
            <div className="t-ui-xs" style={{ marginTop: 6 }}>
              See specifications/ui/SET-project-settings.md SET-FR-04 through SET-FR-08 for the
              full surface.
            </div>
          </div>
          )}
      </div>
    </div>
  );
}

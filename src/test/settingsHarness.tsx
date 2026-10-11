import { SettingsBody } from "../components/SettingsBody";
import type { GlobalSettingsProps } from "../components/GlobalSettings";
import type { SettingsProps } from "../components/Settings";
import type { SectionRequest } from "../state/settingsSweep";

/**
 * Mounts the Settings window's body (`specifications/ui/SWN-settings-windows.md`)
 * with one group, so a test of a section drives it through the real side nav
 * without the native window around it.
 *
 * The window itself — its context read, its events, its save sweep — is the
 * subject of `SettingsWindowApp.test.tsx`; the section tests only need the nav
 * and the content. Each group is offered alone, so the sections the two groups
 * share by name (Plugins, Agents, Docker, and the adapters) do not make a query
 * by name ambiguous.
 */
type GlobalHarnessProps = Omit<GlobalSettingsProps, "section" | "active"> & {
  sectionRequest?: SectionRequest | null;
};

type ProjectHarnessProps = Omit<SettingsProps, "section" | "active"> & {
  sectionRequest?: SectionRequest | null;
};

/** The Global group, on Appearance unless a request says otherwise. */
export function GlobalSettings({
  sectionRequest = null,
  ...global
}: GlobalHarnessProps) {
  // AGT-FR-06 / AGT-FR-07: an editor request names the Agents section, exactly
  // as the window derives it from the section address that carried the request.
  const request: SectionRequest | null =
    sectionRequest ??
    (global.agentEditorRequest
      ? { section: "global/agents", nonce: global.agentEditorRequest.nonce }
      : null);
  return (
    <div style={{ height: 600 }}>
      <SettingsBody
        scopes={["global"]}
        sectionRequest={request}
        global={global}
      />
    </div>
  );
}

/** The Project group, on its first section unless a request says otherwise. */
export function Settings({
  sectionRequest = null,
  ...project
}: ProjectHarnessProps) {
  // AGT-FR-06 / AGT-FR-07: an editor request names the Project Agents section.
  const request: SectionRequest | null =
    sectionRequest ??
    (project.agentEditorRequest
      ? { section: "project/agents", nonce: project.agentEditorRequest.nonce }
      : null);
  return (
    <div style={{ height: 600 }}>
      <SettingsBody
        scopes={["project"]}
        sectionRequest={request}
        global={{}}
        project={project}
      />
    </div>
  );
}

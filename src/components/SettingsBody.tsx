import { useCallback, useEffect, useId, useRef, useState } from "react";

import {
  firstSectionOf,
  isSectionOf,
  parseSectionAddress,
  type SettingsScope,
} from "../settingsWindow";
import type { SectionRequest } from "../state/settingsSweep";
import {
  GLOBAL_SECTIONS,
  GlobalSettings,
  type GlobalSectionKey,
  type GlobalSettingsProps,
} from "./GlobalSettings";
import {
  PROJECT_SECTIONS,
  Settings,
  type ProjectSectionKey,
  type SettingsProps,
} from "./Settings";
import {
  SettingsSectionNav,
  settingsGroupLabelId,
  settingsPanelId,
  settingsTabId,
  type SettingsNavGroup,
  type SettingsSelection,
} from "./SettingsSectionNav";
import type { GraduationFocusRequest } from "./GraduationLoopSliders";

/**
 * The body of the Settings window: the side nav with its two groups, and the
 * content of the section on screen
 * (`../../specifications/ui/SWN-settings-windows.md` SWN-FR-QXTV, SWN-FR-MRNB,
 * SWN-FR-HZWD).
 *
 * The window chooses the section on screen, because the nav spans both groups;
 * `GlobalSettings` (`GLS-global-settings.md`) and `Settings`
 * (`SET-project-settings.md`) render the content of the section they are told
 * to present. **Both stay mounted** whichever group is on screen: what they hold
 * is a pending change the window's save sweep reaches from every section of both
 * groups (SWN-FR-08), and a group that unmounted with the other on screen would
 * take its pending change with it.
 */

const GROUP_LABELS: Record<SettingsScope, string> = {
  global: "Global",
  project: "Project",
};

/** SWN-FR-QXTV: the groups the nav lists, in order, for the scopes on offer. */
function groupsFor(scopes: ReadonlyArray<SettingsScope>): SettingsNavGroup[] {
  return scopes.map((scope) => ({
    scope,
    label: GROUP_LABELS[scope],
    sections: scope === "global" ? GLOBAL_SECTIONS : PROJECT_SECTIONS,
  }));
}

/**
 * The section an address names, if the window offers it (SWN-FR-HZWD,
 * SWN-FR-16). An address naming a scope the window does not offer — the Project
 * group while no project is open — or a section it does not have names nothing.
 */
export function resolveSelection(
  address: string | null,
  scopes: ReadonlyArray<SettingsScope>,
): SettingsSelection | null {
  if (!address) return null;
  const parsed = parseSectionAddress(address);
  if (!parsed || !scopes.includes(parsed.scope)) return null;
  return isSectionOf(parsed.scope, parsed.section)
    ? { scope: parsed.scope, section: parsed.section }
    : null;
}

/**
 * GRU-FR-QLRQ / GRU-FR-TTLR: the Graduation control an address names, for
 * example `project/graduation:total-time-limit`, or null where the address
 * names no control of that section, or the window does not offer it.
 */
export function graduationFocusOf(
  request: SectionRequest | null,
  scopes: ReadonlyArray<SettingsScope>,
): GraduationFocusRequest | null {
  if (!request) return null;
  const parsed = parseSectionAddress(request.section);
  if (
    !parsed ||
    parsed.scope !== "project" ||
    !scopes.includes("project") ||
    parsed.section !== "graduation" ||
    parsed.item === null
  ) {
    return null;
  }
  return { item: parsed.item, nonce: request.nonce };
}

/** SWN-FR-MRNB: the first section of the first group. */
function firstSelection(scopes: ReadonlyArray<SettingsScope>): SettingsSelection {
  const scope = scopes[0] ?? "global";
  return { scope, section: firstSectionOf(scope) };
}

export interface SettingsBodyProps {
  /** The groups on offer, in order; the Project group is absent without a project. */
  scopes: ReadonlyArray<SettingsScope>;
  /**
   * SWN-FR-06 / SWN-FR-11 / SWN-FR-HZWD: present a named section — the one a
   * request routed to, or the one whose save failed. The same address asked for
   * twice presents twice (the nonce), and one that names nothing on offer
   * changes nothing.
   */
  sectionRequest: SectionRequest | null;
  global: Omit<GlobalSettingsProps, "section" | "active">;
  /** SWN-FR-FCRT: put the keyboard focus inside the window when it mounts. */
  autoFocus?: boolean;
  /** Needed only where `scopes` holds the Project group. */
  project?: Omit<
    SettingsProps,
    "section" | "active" | "graduationFocusRequest" | "onGraduationFocusHandled"
  >;
}

export function SettingsBody({
  scopes,
  sectionRequest,
  global,
  project,
  autoFocus = false,
}: SettingsBodyProps) {
  // SWN-FR-MRNB: a request that names no section opens on Appearance. The first
  // request is read before the first paint, so the window is never seen on one
  // section and moved to another.
  const [selected, setSelected] = useState<SettingsSelection>(
    () => resolveSelection(sectionRequest?.section ?? null, scopes) ?? firstSelection(scopes),
  );
  // GRU-FR-QLRQ / GRU-FR-TTLR: the Graduation slider a request named, held
  // until the section has given it the focus once.
  const [graduationFocus, setGraduationFocus] = useState<GraduationFocusRequest | null>(
    () => graduationFocusOf(sectionRequest, scopes),
  );
  const handleGraduationFocus = useCallback(() => setGraduationFocus(null), []);
  const handledNonce = useRef(sectionRequest?.nonce ?? null);
  useEffect(() => {
    if (!sectionRequest || sectionRequest.nonce === handledNonce.current) return;
    handledNonce.current = sectionRequest.nonce;
    const wanted = resolveSelection(sectionRequest.section, scopes);
    if (wanted) setSelected(wanted);
    setGraduationFocus(graduationFocusOf(sectionRequest, scopes));
  }, [sectionRequest, scopes]);

  const idBase = useId();
  const groups = groupsFor(scopes);
  return (
    <div
      style={{ display: "grid", gridTemplateColumns: "200px 1fr", height: "100%" }}
    >
      <SettingsSectionNav
        idBase={idBase}
        groups={groups}
        selected={selected}
        onSelect={setSelected}
        autoFocus={autoFocus}
      />
      {/* SWN-FR-KBNV: one region for the section on screen, named by its
          group and its section, so a section that exists in both groups is told
          apart without sight of the divider. A grid with one bounded row keeps
          the section filling the space, as it did as a track of the body. */}
      <div
        role="tabpanel"
        id={settingsPanelId(idBase)}
        aria-labelledby={`${settingsGroupLabelId(idBase, selected.scope)} ${settingsTabId(idBase, selected.scope, selected.section)}`}
        style={{
          display: "grid",
          gridTemplateRows: "minmax(0, 1fr)",
          minWidth: 0,
          minHeight: 0,
        }}
      >
        {scopes.includes("global") && (
          <GlobalSettings
            {...global}
            section={
              (selected.scope === "global"
                ? selected.section
                : "appearance") as GlobalSectionKey
            }
            active={selected.scope === "global"}
          />
        )}
        {scopes.includes("project") && project && (
          <Settings
            {...project}
            graduationFocusRequest={graduationFocus}
            onGraduationFocusHandled={handleGraduationFocus}
            section={
              (selected.scope === "project"
                ? selected.section
                : "project") as ProjectSectionKey
            }
            active={selected.scope === "project"}
          />
        )}
      </div>
    </div>
  );
}

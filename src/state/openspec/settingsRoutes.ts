/**
 * The two settings routes of an OpenSpec failure notice
 * (`../../../specifications/ui/OFN-openspec-failure-notices.md` OFN-FR-EPIG):
 * the OpenSpec section of Project settings (SET-FR-FZKL), and the Docker
 * section (SET-FR-21) for the agent image.
 *
 * Every surface opens these sections through this module alone.
 */
import { openSettingsWindow } from "../../api";
import { logInfo, logWarn } from "../../logging";
import { formatSectionAddress } from "../../settingsWindow";

function open(section: "openspec" | "docker"): void {
  logInfo(["frontend"], "an openspec notice opens project settings", { section });
  void openSettingsWindow(formatSectionAddress("project", section)).catch((e: unknown) => {
    logWarn(["frontend"], "Settings window could not be opened", {
      section,
      reason: e instanceof Error ? e.name : typeof e,
    });
  });
}

/** OFN-FR-EPIG: open the OpenSpec section of Project settings. */
export function openOpenSpecSettings(): void {
  open("openspec");
}

/** OFN-FR-EPIG: open the Docker section of Project settings. */
export function openDockerSettings(): void {
  open("docker");
}

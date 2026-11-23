/**
 * The link opener of a surface (`ASF-agent-surfaces.md` ASF-FR-QXYW,
 * ASF-FR-VZCC): a link of a `Text` and the address of `openUrl` open through
 * the application's opener, never in the window.
 */
import { openUrl } from "@tauri-apps/plugin-opener";

import { logWarn } from "../../../logging";

/** AUI-FR-GUUJ: the addresses a surface may open. */
export function isOpenableLink(url: string): boolean {
  return /^(https?:\/\/|mailto:)./i.test(url);
}

/** ASF-FR-VZCC: open `url` through the application's opener. */
export function openLink(url: string): void {
  if (!isOpenableLink(url)) {
    // ASF-FR-MGVD: a known scheme name, never the address.
    const scheme = /^([a-z][a-z0-9+.-]{0,15}):/i.exec(url)?.[1]?.toLowerCase() ?? "none";
    logWarn(["frontend"], "agent surface link refused", { scheme });
    return;
  }
  void Promise.resolve(openUrl(url)).catch(() => {
    logWarn(["frontend"], "agent surface link could not be opened", {});
  });
}

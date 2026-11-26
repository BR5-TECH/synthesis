/**
 * The links of a terminal (`../../specifications/ui/TRM-terminals.md`
 * TRM-FR-HWKO, TRM-FR-LNOZ).
 *
 * ghostty-web 0.4.0 finds OSC 8 hyperlinks and URLs in the text, and on
 * Cmd/Ctrl+click it calls `window.open`. The WebView opens no window for that
 * call, so the link does nothing. This module gives each link that the library
 * finds an activation that opens the address through the application's opener.
 */
import type { ILink } from "ghostty-web";
import { openUrl } from "@tauri-apps/plugin-opener";
import { logDebug, logWarn } from "../logging";
import { isMacPlatform } from "../platform";

/** The part of the library's link detector that this module uses (not in the public types). */
interface LinkDetectorInternals {
  getLinkAt?: (col: number, row: number) => Promise<ILink | undefined>;
}

interface TerminalInternals {
  linkDetector?: LinkDetectorInternals;
}

/** TRM-FR-HWKO: the addresses that a terminal link may open. */
export function isBrowserLink(url: string): boolean {
  return /^https?:\/\/./i.test(url);
}

/** The schemes that a record names. A record names another scheme "other". */
const KNOWN_SCHEMES = new Set(["http", "https", "mailto", "ftp", "ssh", "git", "tel", "magnet", "gemini", "gopher", "news", "file", "javascript", "data"]);

/**
 * TRM-FR-LNOZ: the scheme of `url` for a record, never the address. An OSC 8
 * link can have any text before its colon, so only a known scheme name goes
 * into the record.
 */
function schemeOf(url: string): string {
  const scheme = /^([a-z][a-z0-9+.-]{0,15}):/i.exec(url)?.[1]?.toLowerCase();
  if (!scheme) return "none";
  return KNOWN_SCHEMES.has(scheme) ? scheme : "other";
}

/** TRM-FR-HWKO: whether `event` carries the platform's command modifier. */
function hasCommandModifier(event: MouseEvent, mac: boolean): boolean {
  return mac ? event.metaKey : event.ctrlKey;
}

/**
 * TRM-FR-HWKO: opens `url` in the system browser for a click with the
 * platform's command modifier. Other clicks and other schemes open nothing.
 */
export function activateTerminalLink(url: string, event: MouseEvent, terminalId: string, mac = isMacPlatform()): void {
  if (!hasCommandModifier(event, mac)) return;
  const scheme = schemeOf(url);
  if (!isBrowserLink(url)) {
    logDebug(["frontend"], "terminal link refused", { terminalId, scheme });
    return;
  }
  logDebug(["frontend"], "terminal link opened", { terminalId, scheme });
  void Promise.resolve()
    .then(() => openUrl(url))
    .catch(() => {
      logWarn(["frontend"], "terminal link could not be opened", { terminalId, scheme });
    });
}

/**
 * TRM-FR-HWKO: makes each link of `term` open through the application's
 * opener. Call it after `open()`, which makes the link detector. Where the
 * library has no such detector, it changes nothing and, where `warn` is true,
 * logs a `WARN` record (TRM-FR-LNOZ). Returns whether it applied.
 */
export function openLinksInBrowser(term: object, terminalId: string, warn = true): boolean {
  const detector = (term as unknown as TerminalInternals).linkDetector;
  const original = detector?.getLinkAt;
  if (!detector || typeof original !== "function") {
    if (warn) logWarn(["frontend"], "terminal links not applied", { terminalId, reason: "no_link_hook" });
    return false;
  }
  // The library compares the hovered link by identity, so each link changes in
  // place, and only once.
  const changed = new WeakSet<ILink>();
  detector.getLinkAt = async function getBrowserLinkAt(this: LinkDetectorInternals, col, row) {
    const link = await original.call(this, col, row);
    if (link && !changed.has(link)) {
      changed.add(link);
      const url = link.text;
      link.activate = (event: MouseEvent) => activateTerminalLink(url, event, terminalId);
    }
    return link;
  };
  return true;
}

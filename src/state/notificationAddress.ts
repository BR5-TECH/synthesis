/**
 * The `synthesis://` address space (`NTF-notifications.md` NTF-FR-03 through
 * NTF-FR-06).
 *
 * An address names a project, one of its worktrees, and exactly one target
 * within it. It is minted by the surface that raises a notification and parsed
 * only here; the backend carries it as an opaque string and never looks at it
 * (`../core/NTD-notification-delivery.md` NTD-FR-08), and nothing outside this
 * application can mint one or hand one in (NTD-FR-20) — no URL scheme is
 * registered with the operating system.
 *
 * An address is a **value, not a live handle** (NTF-FR-05): it holds no
 * reference to a tab, a buffer, an editing session, a thread, or a run, so it
 * costs nothing to keep and one minted an hour ago parses exactly as one minted
 * a moment ago.
 *
 * Parsing is deliberately hand-rolled rather than `new URL()`. `URL` treats the
 * segment after `//` as a *host* and lowercases it, so a project key or a
 * worktree path carrying an uppercase letter would round-trip to a different
 * address — and would then fail to match the open project for reasons nothing
 * in the UI could explain.
 */
import type { Tab } from "../types";

/** The vertical-panel surfaces an address can name (NTF-FR-03, SNV-FR-44). */
export const PANEL_TARGETS = [
  "library",
  "documents",
  "notes",
  "comments",
  "drafts",
  "changes",
] as const;
export type PanelTarget = (typeof PANEL_TARGETS)[number];

/** The bottom-panel surfaces an address can name (NTF-FR-03, SNV-FR-44). */
export const BOTTOM_TARGETS = ["runs", "logs", "git", "history"] as const;
export type BottomTarget = (typeof BOTTOM_TARGETS)[number];

/** The two settings windows an address can name (NTF-FR-03). */
export const SETTINGS_TARGETS = ["global", "project"] as const;
export type SettingsTarget = (typeof SETTINGS_TARGETS)[number];

/**
 * What an address points at.
 *
 * A Search results, History detail, or Diff tab is deliberately absent
 * (NTF-FR-03): each is a reading the author composes rather than a place work
 * happens, so nothing would ever raise about one.
 */
export type NotificationTarget =
  | { kind: "dashboard" }
  | { kind: "file"; path: string }
  | { kind: "draft"; draftId: string }
  /**
   * NTF-FR-03 / GRU-FR-BLSS: one graduation run. Activating it opens the bottom
   * panel on the Runs surface's graduation section with that run selected.
   *
   * A run is not a file and has no tab of its own (NTF-FR-28): it is a place
   * work is happening, which is exactly what an address may name.
   */
  | { kind: "run"; runId: string }
  | { kind: "panel"; surface: PanelTarget }
  | { kind: "bottom"; surface: BottomTarget }
  | { kind: "settings"; which: SettingsTarget };

/** A parsed address: which content root, and what within it. */
export interface NotificationAddress {
  /**
   * The project's anchor, keyed exactly as the per-project settings slot is
   * (`../core/GSS-global-settings-storage.md` GSS-FR-18) — the repository's
   * primary worktree, so a repository with several worktrees yields one key.
   */
  projectKey: string;
  /** The worktree's path — the content root the target is read from. */
  worktree: string;
  target: NotificationTarget;
}

const SCHEME = "synthesis://";

/**
 * Encode one path component. `encodeURIComponent` leaves `!'()*` alone, which is
 * harmless here, but it does escape `/` — which is exactly what keeps a project
 * key or draft id containing a slash from splitting into two segments.
 */
const encodeSegment = (value: string) => encodeURIComponent(value);

/**
 * A project-relative path keeps its separators (they are structure) while every
 * segment between them is escaped, so a filename containing `/`-adjacent
 * oddities — spaces, `#`, `?`, a literal `%` — survives the round trip.
 */
const encodePath = (path: string) => path.split("/").map(encodeSegment).join("/");

/**
 * NTF-FR-03: build the address for a target within a worktree of a project.
 *
 * Every argument is taken as given; this function validates nothing about
 * whether the project is open or the file exists. That is `resolve`'s job at
 * activation time (NTF-FR-19), and deliberately not the raiser's — a run that
 * finishes after its file was deleted still raised legitimately.
 */
export function mintAddress(
  projectKey: string,
  worktree: string,
  target: NotificationTarget,
): string {
  const head = `${SCHEME}${encodeSegment(projectKey)}/${encodeSegment(worktree)}`;
  switch (target.kind) {
    case "dashboard":
      return `${head}/dashboard`;
    case "file":
      return `${head}/file/${encodePath(target.path)}`;
    case "draft":
      return `${head}/draft/${encodeSegment(target.draftId)}`;
    case "run":
      return `${head}/run/${encodeSegment(target.runId)}`;
    case "panel":
      return `${head}/panel/${target.surface}`;
    case "bottom":
      return `${head}/bottom/${target.surface}`;
    case "settings":
      return `${head}/settings/${target.which}`;
  }
}

const decodeSegment = (value: string): string | null => {
  try {
    return decodeURIComponent(value);
  } catch {
    // A lone `%` or a truncated escape. An address this malformed is one the
    // facility cannot parse, which routes exactly as an unreachable one does
    // (NTF-FR-19) rather than throwing into the activation handler.
    return null;
  }
};

const isOneOf = <T extends string>(
  values: readonly T[],
  candidate: string,
): candidate is T => (values as readonly string[]).includes(candidate);

/**
 * NTF-FR-03: parse an address, or `null` when it is not one.
 *
 * Returning `null` rather than throwing is what lets NTF-FR-19 treat a
 * malformed payload and an unreachable target as the same case — both leave the
 * window as it is and state that nothing could be reached.
 */
export function parseAddress(raw: string): NotificationAddress | null {
  if (typeof raw !== "string" || !raw.startsWith(SCHEME)) return null;
  const rest = raw.slice(SCHEME.length);
  if (rest === "") return null;

  const parts = rest.split("/");
  // At minimum: project, worktree, and a target kind.
  if (parts.length < 3) return null;

  const projectKey = decodeSegment(parts[0]);
  const worktree = decodeSegment(parts[1]);
  if (projectKey === null || worktree === null) return null;
  if (projectKey === "" || worktree === "") return null;

  const kind = parts[2];
  const tail = parts.slice(3);

  const address = (target: NotificationTarget): NotificationAddress => ({
    projectKey,
    worktree,
    target,
  });

  switch (kind) {
    case "dashboard":
      // A trailing segment means the address says more than this target can
      // carry, so it is not this address rather than a lenient match.
      return tail.length === 0 ? address({ kind: "dashboard" }) : null;
    case "file": {
      if (tail.length === 0) return null;
      const decoded = tail.map(decodeSegment);
      if (decoded.some((segment) => segment === null)) return null;
      const path = (decoded as string[]).join("/");
      return path === "" ? null : address({ kind: "file", path });
    }
    case "draft": {
      if (tail.length !== 1) return null;
      const draftId = decodeSegment(tail[0]);
      return draftId ? address({ kind: "draft", draftId }) : null;
    }
    case "run": {
      if (tail.length !== 1) return null;
      const runId = decodeSegment(tail[0]);
      return runId ? address({ kind: "run", runId }) : null;
    }
    case "panel": {
      if (tail.length !== 1) return null;
      return isOneOf(PANEL_TARGETS, tail[0])
        ? address({ kind: "panel", surface: tail[0] })
        : null;
    }
    case "bottom": {
      if (tail.length !== 1) return null;
      return isOneOf(BOTTOM_TARGETS, tail[0])
        ? address({ kind: "bottom", surface: tail[0] })
        : null;
    }
    case "settings": {
      if (tail.length !== 1) return null;
      return isOneOf(SETTINGS_TARGETS, tail[0])
        ? address({ kind: "settings", which: tail[0] })
        : null;
    }
    default:
      return null;
  }
}

/**
 * NTF-FR-27: what an open tab is a view onto, when that is something an address
 * can name.
 *
 * This is the *one* mapping from a tab to a target, and both directions of the
 * facility read it: the post policy asks it what the active tab is showing
 * (NTF-FR-08), and the indication asks it which open tab an address's own
 * routing would activate (NTF-FR-27). Deriving them separately is how the two
 * would drift into disagreeing about whether the author is looking at something
 * — and a disagreement there marks the very tab they are reading.
 *
 * A Diff tab, a History detail tab, a Search results tab, and a conversation tab
 * each yield null. None of the four is addressable (NTF-FR-03), and a Diff or
 * History tab open on a file whose Editor tab is also open is deliberately not
 * the one marked: activation lands on the editing tab, so that is the tab the
 * indication belongs to (per `TAB-tabs.md` TAB-FR-06).
 *
 * A `settings` address has no tab either, and no branch here: the two settings
 * surfaces are native child windows rather than tabs of the main viewport
 * (per `SWN-settings-windows.md` SWN-FR-01), so such an address marks nothing
 * anywhere and is reached through its notification alone (NTF-FR-27,
 * NTF-FR-28). Whether the author is already looking at one is a question about
 * a window, and `notifications.ts` answers it from the window snapshot.
 */
export function targetForTab(tab: Tab): NotificationTarget | null {
  if (tab.id === "dashboard") return { kind: "dashboard" };
  // TAB-FR-17: a draft is identified by its own id rather than by a path.
  if (tab.kind === "draft")
    return tab.draftId ? { kind: "draft", draftId: tab.draftId } : null;
  if (tab.kind === "editor" || tab.kind === "flow")
    return tab.artifactId ? { kind: "file", path: tab.artifactId } : null;
  return null;
}

/** Whether two targets name the same thing. */
export function sameTarget(
  a: NotificationTarget | null,
  b: NotificationTarget | null,
): boolean {
  if (!a || !b || a.kind !== b.kind) return false;
  switch (a.kind) {
    case "dashboard":
      return true;
    case "file":
      return a.path === (b as { path: string }).path;
    case "draft":
      return a.draftId === (b as { draftId: string }).draftId;
    case "run":
      return a.runId === (b as { runId: string }).runId;
    case "panel":
    case "bottom":
      return a.surface === (b as { surface: string }).surface;
    case "settings":
      return a.which === (b as { which: string }).which;
  }
}

/** The strip as an address is resolved against. */
export interface StripSnapshot {
  tabs: Tab[];
  activeTab: string;
  projectKey: string | null;
  worktree: string | null;
}

/**
 * NTF-FR-27 / NTF-FR-28: the open tab an address's own routing would activate,
 * or null when nothing in the strip is a view onto it.
 *
 * The single answer to "does this address have a tab?", asked from both places
 * that need it — the facility, deciding at raise time whether to mark, and the
 * reconciliation that drops indications whose tabs have gone (NTF-FR-33). Asked
 * two different ways, the two would eventually disagree, and the disagreement
 * would show up as an indication that can never be cleared because nothing
 * believes its tab exists.
 *
 * The root check is part of it rather than the caller's: an address into a
 * worktree the application is not reading has no open tab here, whatever this
 * strip happens to hold at the same path.
 */
export function resolveTabForAddress(
  address: NotificationAddress,
  strip: StripSnapshot,
): { tabId: string; active: boolean } | null {
  if (
    address.projectKey !== strip.projectKey ||
    address.worktree !== strip.worktree
  ) {
    return null;
  }
  const tab = strip.tabs.find((t) =>
    sameTarget(targetForTab(t), address.target),
  );
  return tab ? { tabId: tab.id, active: tab.id === strip.activeTab } : null;
}

/**
 * Whether two addresses name the same content root — the test NTF-FR-19 applies
 * before it will route anything.
 */
export function sameRoot(
  address: NotificationAddress,
  projectKey: string,
  worktree: string,
): boolean {
  return address.projectKey === projectKey && address.worktree === worktree;
}

/**
 * A short, author-facing name for what an address points at, for the statement
 * of NTF-FR-20 to say what could not be reached.
 *
 * Deliberately not the whole address: an author reads "that artifact is no
 * longer in the project", not a URI. A file is named by its basename because
 * that is what they would recognise from the tab strip.
 */
export function describeTarget(target: NotificationTarget): string {
  switch (target.kind) {
    case "dashboard":
      return "the Dashboard";
    case "file": {
      const name = target.path.split("/").filter(Boolean).pop();
      return name ?? target.path;
    }
    case "draft":
      return "that draft";
    case "run":
      return "that graduation run";
    case "panel":
      return `the ${target.surface} panel`;
    case "bottom":
      return `the ${target.surface} panel`;
    case "settings":
      return target.which === "global" ? "Global settings" : "Project settings";
  }
}

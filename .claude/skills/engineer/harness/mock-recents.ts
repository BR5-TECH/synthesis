/*
 * Recent projects stand-in (GSS-global-settings-storage.md GSS-FR-05 to
 * GSS-FR-09, PPK-project-picker.md), for UI auditing only.
 *
 * The list keeps its state: Pin, Unpin, Hide, and Clear change it, and the next
 * "list recent projects" shows the change. State a flag can reach, on the URL:
 *   ?manyRecents     the list overflows and scrolls (PPK-FR-11)
 */

import { NOT_HANDLED } from "./mock-github-polling";

const harnessFlag = (name: string) =>
  new URLSearchParams(location.search).has(name);

type MockRecent = {
  name: string;
  path: string;
  lastOpenedAt: string;
  pinned?: boolean;
  missing?: boolean;
};

/**
 * The recent list, with state: Pin, Unpin, and Hide in the picker change it,
 * and the next "list recent projects" shows the change. `?manyRecents` adds
 * entries, so that the list overflows and scrolls (PPK-FR-11).
 */
let mockRecents: MockRecent[] = [
  {
    name: "acme",
    path: "/Users/demo/dev/acme",
    lastOpenedAt: "2026-07-28T09:00:00Z",
    pinned: true,
  },
  {
    name: "synthesis",
    path: "/Users/demo/develop/synthesis-june",
    lastOpenedAt: "2026-07-24T18:30:00Z",
  },
  {
    name: "old-thing",
    path: "/Users/demo/dev/old-thing",
    lastOpenedAt: "2026-03-01T12:00:00Z",
    missing: true,
  },
  ...(harnessFlag("manyRecents")
    ? [
        { name: "design-skills", path: "/Users/demo/dev/design-skills", lastOpenedAt: "2026-07-20T10:00:00Z" },
        { name: "flow-prototype", path: "/Users/demo/dev/flow-prototype", lastOpenedAt: "2026-07-10T10:00:00Z" },
        { name: "acme-docs", path: "/Users/demo/dev/acme-docs", lastOpenedAt: "2026-06-30T10:00:00Z" },
        { name: "infra-notes", path: "/Users/demo/dev/infra-notes", lastOpenedAt: "2026-06-12T10:00:00Z" },
        { name: "website", path: "/Users/demo/dev/website", lastOpenedAt: "2026-05-02T10:00:00Z" },
        { name: "billing-api", path: "/Users/demo/dev/billing-api", lastOpenedAt: "2026-04-21T10:00:00Z" },
        { name: "mobile-app", path: "/Users/demo/dev/mobile-app", lastOpenedAt: "2026-04-09T10:00:00Z" },
        { name: "research", path: "/Users/demo/dev/research", lastOpenedAt: "2026-03-28T10:00:00Z" },
        { name: "sandbox", path: "/Users/demo/dev/sandbox", lastOpenedAt: "2026-03-15T10:00:00Z" },
      ]
    : []),
];

/** Pinned entries first, then the most recent first (GSS-FR-08). */
function orderedRecents(): MockRecent[] {
  return [...mockRecents].sort((x, y) => {
    if (!!x.pinned !== !!y.pinned) return x.pinned ? -1 : 1;
    return y.lastOpenedAt.localeCompare(x.lastOpenedAt);
  });
}

function setRecentPinned(path: string, pinned: boolean) {
  mockRecents = mockRecents.map((r) => (r.path === path ? { ...r, pinned } : r));
}

/** Handle one recent-projects command, or answer NOT_HANDLED. */
export function recentsInvoke(
  cmd: string,
  a: Record<string, unknown>,
): unknown {
  switch (cmd) {
    case "list_recent_projects":
      return orderedRecents();
    // GSS-FR-08 / PPK-FR-YNME / PPK-FR-MQVU: the recent list keeps its state.
    case "pin_recent_project":
      setRecentPinned(a.path as string, true);
      return undefined;
    case "unpin_recent_project":
      setRecentPinned(a.path as string, false);
      return undefined;
    case "remove_recent_project":
      mockRecents = mockRecents.filter((r) => r.path !== a.path);
      return undefined;
    case "clear_recent_projects":
      mockRecents = [];
      return undefined;
    default:
      return NOT_HANDLED;
  }
}

/*
 * Map thread stand-in (MPR-map-plan-proposals.md), for UI auditing only.
 *
 * The threads of the Map tab live in memory. `open_map_discussion` always makes
 * a new thread (MPR-FR-MAXH), `find_map_discussion` answers the latest open
 * thread of the node (MPR-FR-MYLA), and `list_map_discussions` answers every
 * thread in the order it was opened (MPR-FR-NRNZ). `add_comment`,
 * `set_discussion_resolution` and `read_discussion` serve a Map thread here and
 * fall through for every other discussion. Starts with no thread.
 */

import { NOT_HANDLED } from "./mock-github-polling";

const ME = { kind: "human", login: "" };

type MapNode = { lens: string; kind: string; id: string; label: string } | null;

const threads: any[] = [];
let nextThread = 1;

const sameNode = (a: MapNode, b: MapNode) =>
  (a ?? null) === null || (b ?? null) === null
    ? (a ?? null) === (b ?? null)
    : a!.lens === b!.lens && a!.kind === b!.kind && a!.id === b!.id;

const own = (id: unknown) => threads.find((t) => t.id === id);

function comment(thread: any, a: Record<string, any>, at: string) {
  return {
    id: `${thread.id}-c${thread.comments.length + 1}`,
    author: ME,
    body: String(a.body ?? ""),
    quotes: a.quotes ?? [],
    attachments: a.attachments ?? [],
    createdAt: at,
  };
}

/** Handle one Map thread command, or answer NOT_HANDLED. */
export function mapDiscussionsInvoke(cmd: string, a: Record<string, any>): unknown {
  switch (cmd) {
    case "list_map_discussions":
      return threads.map((t) => ({ ...t }));
    case "find_map_discussion": {
      const open = threads.filter((t) => !t.resolved && sameNode(t.mapNode ?? null, a.mapNode ?? null));
      const latest = [...open].sort((x, y) => x.updatedAt.localeCompare(y.updatedAt)).pop();
      return latest ? { ...latest } : null;
    }
    case "open_map_discussion": {
      const at = new Date().toISOString();
      const made: any = {
        id: `map-${nextThread++}`,
        target: { kind: "map" },
        fragmentTarget: null,
        mapNode: a.mapNode ?? null,
        comments: [],
        resolved: false,
        createdAt: at,
        updatedAt: at,
      };
      made.comments.push(comment(made, a, at));
      threads.push(made);
      return { ...made };
    }
    case "add_comment": {
      const t = own(a.discussionId);
      if (!t) return NOT_HANDLED;
      const at = new Date().toISOString();
      t.comments = [...t.comments, comment(t, a, at)];
      t.updatedAt = at;
      return { ...t };
    }
    case "set_discussion_resolution": {
      const t = own(a.discussionId);
      if (!t) return NOT_HANDLED;
      t.resolved = !!a.resolved;
      t.updatedAt = new Date().toISOString();
      return { ...t };
    }
    case "read_discussion": {
      const t = own(a.discussionId);
      return t ? { ...t } : NOT_HANDLED;
    }
    case "list_map_plan_proposals":
      return own(a.discussionId) ? [] : NOT_HANDLED;
    default:
      return NOT_HANDLED;
  }
}

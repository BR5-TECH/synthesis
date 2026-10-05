/**
 * The unified discussion contract (`Discussion` in `src/types/comments.ts`)
 * over the mock's older discussion records.
 *
 * The frontend now names every discussion command by `discussionId` and reads
 * every discussion as `{ id, target, fragmentTarget, comments, locked,
 * resolved, createdAt, updatedAt }`. The seeded records in `mock-core.ts` still
 * carry the earlier `{ scope, kind, artifactId | draftId | noteId, anchor }`
 * shape and are reached by the earlier command names. This adapter sits in
 * front of the legacy `invoke`: it maps the new command names and argument
 * names to the legacy ones, and it converts each discussion the legacy code
 * returns (or fires on the bus) to the unified shape.
 */

type Legacy = (cmd: string, args?: Record<string, any>) => Promise<any>;

/** True for a record in the legacy discussion shape. */
function isLegacyDiscussion(v: unknown): v is Record<string, any> {
  return (
    !!v &&
    typeof v === "object" &&
    Array.isArray((v as any).comments) &&
    typeof (v as any).id === "string" &&
    "scope" in (v as any) &&
    !("target" in (v as any))
  );
}

/** Convert one legacy discussion record to the unified `Discussion` shape. */
export function toUnified(old: any): any {
  if (!isLegacyDiscussion(old)) return old;
  const target =
    old.scope === "draft"
      ? { kind: "draft", draftId: String(old.draftId) }
      : old.scope === "note"
        ? { kind: "note", noteId: String(old.noteId) }
        : { kind: "artifact", artifactId: String(old.artifactId) };
  const anchor = old.anchor ?? null;
  const fragmentTarget = anchor
    ? {
        owner: target,
        path: String(anchor.path ?? old.artifactId ?? ""),
        start: anchor.start,
        end: anchor.end,
        quote: anchor.quote,
      }
    : null;
  return {
    id: old.id,
    target,
    fragmentTarget,
    comments: old.comments,
    locked: !!old.locked,
    resolved: !!old.resolved,
    createdAt: old.createdAt,
    updatedAt: old.updatedAt,
  };
}

/** Convert a result that is a discussion, a list of them, or a wrapper. */
function convert(v: any): any {
  if (Array.isArray(v)) return v.map(convert);
  if (isLegacyDiscussion(v)) return toUnified(v);
  return v;
}

/** Convert a bus event the legacy code fires to the name and shape the app listens for. */
export function toUnifiedEvent(name: string, payload: any): [string, any] {
  if (name === "comment-thread-changed") return ["discussion-changed", toUnified(payload)];
  if (name === "discussion-question-set-changed" && payload && "threadId" in payload) {
    return [name, { discussionId: payload.threadId, set: payload.set }];
  }
  return [name, payload];
}

/** The `invoke` the app calls: the unified names in front of the legacy ones. */
export async function unifiedInvoke(
  cmd: string,
  args: Record<string, any> | undefined,
  legacy: Legacy,
): Promise<any> {
  const a: Record<string, any> = { ...(args ?? {}) };
  if (typeof a.discussionId === "string" && a.threadId === undefined) {
    a.threadId = a.discussionId;
  }
  switch (cmd) {
    case "list_discussions": {
      const t = a.target ?? {};
      const whole = (await legacy("list_discussion_threads", { target: t })) ?? [];
      const fragments =
        t.kind === "artifact"
          ? ((await legacy("list_comment_threads", { artifactId: t.artifactId })) ?? [])
          : [];
      return convert([...fragments, ...whole]);
    }
    case "list_all_discussions": {
      const rows = (await legacy("list_all_comment_threads", {})) ?? [];
      return rows.map((r: any) => ({
        discussion: toUnified(r.thread),
        ownerUnavailable: false,
      }));
    }
    case "read_discussion":
      return convert(await legacy("read_comment_thread", a));
    case "open_discussion": {
      const ft = a.fragmentTarget ?? null;
      if (ft) {
        return convert(
          await legacy("open_comment_thread", {
            ...a,
            artifactId: a.target?.artifactId ?? ft.path,
            anchor: { start: ft.start, end: ft.end, quote: ft.quote },
          }),
        );
      }
      return convert(await legacy("open_discussion_thread", a));
    }
    case "set_discussion_lock":
      return convert(await legacy("set_comment_thread_lock", a));
    case "set_discussion_resolution":
      return convert(await legacy("set_comment_thread_resolution", a));
    case "reanchor_discussion_fragment": {
      const ft = a.fragmentTarget ?? {};
      return convert(
        await legacy("reanchor_comment_thread", {
          ...a,
          anchor: { start: ft.start, end: ft.end, quote: ft.quote },
        }),
      );
    }
    case "submit_discussion_question_answers": {
      const r = await legacy(cmd, a);
      return r && r.thread
        ? { discussion: toUnified(r.thread), finalAnswerCommentId: r.finalAnswerCommentId }
        : r;
    }
    default:
      return convert(await legacy(cmd, a));
  }
}

import { expect } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";

import type { Mock } from "vitest";

import type {
  ArtifactContents,
  Attachment,
  AttachmentInput,
  Discussion,
} from "../types";

export const BODY = "# Onboarding\n\nSteps to run before the first session.\n";

export function human(login: string) {
  return { kind: "human", login } as const;
}

/** CMS-FR-HTOA: the participant a project without a GitHub token writes as. */
export const LOCAL_HUMAN = { kind: "human", login: "", displayName: "Me" } as const;

export { fragment } from "./discussionFixtures";

export function makeThread(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "t1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: {
      owner: { kind: "artifact", artifactId: "a.md" },
      path: "a.md",
      start: BODY.indexOf("the first session"), end: 0, quote: "the first session",
    },
    comments: [
      {
        id: "c1",
        author: human("raver119"),
        body: "Which session? Say **which** one.",
        quotes: [],
        attachments: [],
        createdAt: "2026-01-01T00:00:00Z",
      },
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

export interface Backend {
  load: ArtifactContents;
  threads: Discussion[];
  identity?: unknown;
  identityError?: string;
  calls: { cmd: string; args: unknown }[];
  onOpenThread?: (args: unknown) => Discussion;
  addCommentError?: string;
  readAttachmentError?: string;
}

export function wireBackend(invokeMock: Mock, b: Backend) {
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "load_artifact_contents_by_id":
        return b.load;
      case "save_artifact_contents":
        return { checksum: "ck-saved" };
      case "list_discussions":
        return b.threads;
      case "read_comment_attachment": {
        if (b.readAttachmentError) throw b.readAttachmentError;
        // Keyed on the digest, and reporting the media type the log line
        // actually carries. A mock that answered `image/png` for every digest
        // would let a PDF render as a thumbnail and hide the case CMT-FR-48
        // most cares about.
        const { digest } = args as { digest: string };
        const found = b.threads
          .flatMap((t) => t.comments)
          .flatMap((c) => c.attachments)
          .find((a) => a.kind === "blob" && a.digest === digest);
        if (!found || found.kind !== "blob") throw "attachment_not_found";
        return {
          mediaType: found.mediaType,
          filename: found.filename,
          data: "aGVsbG8=",
        };
      }
      case "resolve_comment_author_identity":
        if (b.identityError) throw b.identityError;
        return b.identity ?? human("raver119");
      case "open_discussion": {
        const created = b.onOpenThread
          ? b.onOpenThread(args)
          : makeThread({ id: "t-new" });
        b.threads = [...b.threads, created];
        return created;
      }
      case "add_comment": {
        if (b.addCommentError) throw b.addCommentError;
        const { discussionId: threadId, body, quotes, attachments } = args as {
          discussionId: string;
          body: string;
          quotes: unknown[];
          attachments: AttachmentInput[];
        };
        // CMS-FR-43 / CMS-FR-44: the backend stores an inline payload and hands
        // back a reference; a url is recorded as given.
        const stored: Attachment[] = (attachments ?? []).map((a, i) =>
          a.kind === "inline"
            ? {
                kind: "blob",
                digest: `digest-${i}`,
                mediaType: a.mediaType,
                filename: a.filename,
                bytes: a.data.length,
              }
            : { kind: "url", url: a.url, mediaType: a.mediaType, label: a.label },
        );
        const target = b.threads.find((t) => t.id === threadId)!;
        const updated: Discussion = {
          ...target,
          comments: [
            ...target.comments,
            {
              id: `c-${target.comments.length + 1}`,
              author: human("octocat"),
              body,
              quotes: quotes as never,
              attachments: stored,
              createdAt: "2026-01-02T00:00:00Z",
            },
          ],
        };
        b.threads = b.threads.map((t) => (t.id === threadId ? updated : t));
        return updated;
      }
      case "set_discussion_lock": {
        const { discussionId: threadId, locked } = args as { discussionId: string; locked: boolean };
        const updated = { ...b.threads.find((t) => t.id === threadId)!, locked };
        b.threads = b.threads.map((t) => (t.id === threadId ? updated : t));
        return updated;
      }
      case "set_discussion_resolution": {
        const { discussionId: threadId, resolved } = args as {
          discussionId: string;
          resolved: boolean;
        };
        const updated = { ...b.threads.find((t) => t.id === threadId)!, resolved };
        b.threads = b.threads.map((t) => (t.id === threadId ? updated : t));
        return updated;
      }
      case "reanchor_discussion_fragment": {
        const { discussionId: threadId, fragmentTarget: anchor } = args as { discussionId: string; fragmentTarget: unknown };
        const updated = {
          ...b.threads.find((t) => t.id === threadId)!,
          fragmentTarget: anchor as never,
        };
        b.threads = b.threads.map((t) => (t.id === threadId ? updated : t));
        return updated;
      }
      default:
        throw new Error(`unexpected invoke ${cmd}`);
    }
  });
}

export const commentsToggle = () =>
  screen.getByRole("button", { name: /comments \(\d+ unresolved\)/i });

export async function openRail() {
  await waitFor(() => expect(commentsToggle()).toBeInTheDocument());
  if (commentsToggle().getAttribute("aria-expanded") !== "true") {
    fireEvent.click(commentsToggle());
  }
  return screen.getByRole("complementary", { name: "Comments" });
}

/**
 * Selects `text` where it is rendered inside `el`, as a reader would with the
 * pointer. Unlike `selectInBody` this operates on the real rendered nodes,
 * because what is under test is precisely whether the selection lies inside the
 * comment being quoted.
 */
export function selectWithin(el: Element, text: string) {
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  let node = walker.nextNode();
  while (node) {
    const at = node.textContent?.indexOf(text) ?? -1;
    if (at >= 0) {
      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + text.length);
      const sel = window.getSelection()!;
      sel.removeAllRanges();
      sel.addRange(range);
      return;
    }
    node = walker.nextNode();
  }
  throw new Error(`no rendered text node contains ${JSON.stringify(text)}`);
}

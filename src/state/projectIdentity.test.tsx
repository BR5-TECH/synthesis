/**
 * The label every surface gives the fixed local participant
 * (`CMT-comments.md` CMT-FR-ZCAE, `CVP-conversation-presentation.md`
 * CVP-FR-TIBK) and the shared writer identity that follows token changes
 * (`../../specifications/core/GTS-github-token-storage.md` GTS-FR-AEQO).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, renderHook, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const handlers = new Map<string, Set<() => void>>();
const unlistenMock = vi.fn();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: () => void) => {
    const set = handlers.get(event) ?? new Set();
    set.add(cb);
    handlers.set(event, set);
    return () => {
      set.delete(cb);
      unlistenMock();
    };
  }),
}));

import { resetLogBufferForTest } from "../logging";
import { resetProjectIdentity, useParticipantLabel } from "./projectIdentity";
import {
  resetSharedCommentIdentity,
  useSharedCommentIdentity,
} from "../hooks/useSharedCommentIdentity";
import { isLocalParticipant, participantName, type Participant } from "../types";

const LOCAL: Participant = { kind: "human", login: "", displayName: "Me" };
const GITHUB: Participant = { kind: "human", login: "raver119" };
const AGENT: Participant = { kind: "agent", agentId: "a1", handle: "arch" };

let identity: () => Promise<unknown>;

const tokensChanged = () =>
  act(async () => {
    for (const cb of handlers.get("github-tokens-changed") ?? []) cb();
  });

beforeEach(() => {
  handlers.clear();
  unlistenMock.mockReset();
  invokeMock.mockReset();
  identity = () => Promise.resolve(LOCAL);
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "resolve_comment_author_identity") return identity();
    return undefined;
  });
  resetProjectIdentity();
  resetSharedCommentIdentity();
  // A token change emits a log record, and its flush timer must not reach the
  // next test.
  resetLogBufferForTest();
});
afterEach(cleanup);

describe("CMS-FR-HTOA: recognising the local participant", () => {
  it("is a human with no GitHub login, and nothing else is", () => {
    expect(isLocalParticipant(LOCAL)).toBe(true);
    expect(isLocalParticipant(GITHUB)).toBe(false);
    expect(isLocalParticipant(AGENT)).toBe(false);
    expect(participantName(LOCAL)).toBe("Me");
    expect(participantName(GITHUB)).toBe("raver119");
    expect(participantName(AGENT)).toBe("arch");
  });
});

describe("CVP-FR-TIBK, CMT-FR-ZCAE: the shared label rule", () => {
  it("reads Me for the local participant while no identity resolves and the stamped label for everyone else", async () => {
    const { result } = renderHook(() => useParticipantLabel());
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(result.current(LOCAL)).toBe("Me");
    expect(result.current(GITHUB)).toBe("raver119");
    expect(result.current(AGENT)).toBe("arch");
  });

  it("reads the project login for the local participant once one resolves, never for another participant", async () => {
    identity = () => Promise.resolve({ kind: "human", login: "octocat" });
    const { result } = renderHook(() => useParticipantLabel());
    await waitFor(() => expect(result.current(LOCAL)).toBe("octocat"));
    expect(result.current(GITHUB)).toBe("raver119");
    expect(result.current(AGENT)).toBe("arch");
  });

  it("GTS-FR-AEQO: re-reads on a token change and falls back to Me when the binding is required", async () => {
    const { result } = renderHook(() => useParticipantLabel());
    await waitFor(() => expect(handlers.get("github-tokens-changed")?.size).toBe(1));
    expect(result.current(LOCAL)).toBe("Me");

    identity = () => Promise.resolve({ kind: "human", login: "octocat" });
    await tokensChanged();
    await waitFor(() => expect(result.current(LOCAL)).toBe("octocat"));

    identity = () => Promise.reject("github_token_selection_required");
    await tokensChanged();
    await waitFor(() => expect(result.current(LOCAL)).toBe("Me"));
  });

  it("CVP-FR-49: a project change drops the outgoing project's login and reads the incoming binding", async () => {
    identity = () => Promise.resolve({ kind: "human", login: "octocat" });
    const { result } = renderHook(() => useParticipantLabel());
    await waitFor(() => expect(result.current(LOCAL)).toBe("octocat"));

    identity = () => Promise.resolve(LOCAL);
    act(() => resetProjectIdentity());
    await waitFor(() => expect(result.current(LOCAL)).toBe("Me"));
  });

  it("CVP-FR-49: a read of the outgoing project that lands after the change is not applied, also while nothing renders", async () => {
    let answerOutgoing: (value: unknown) => void = () => {};
    identity = () => new Promise((res) => (answerOutgoing = res));
    const first = renderHook(() => useParticipantLabel());
    await waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "resolve_comment_author_identity")).toBe(true),
    );
    // The last surface goes before the read lands, so the project change
    // starts no read of its own.
    first.unmount();
    act(() => resetProjectIdentity());
    // Drain to a macrotask, so the whole invoke → refresh chain of the stale
    // answer has run before the next surface subscribes and starts its own read.
    await act(async () => {
      answerOutgoing({ kind: "human", login: "octocat" });
      await new Promise((res) => setTimeout(res, 0));
    });

    identity = () => new Promise(() => {});
    const { result } = renderHook(() => useParticipantLabel());
    expect(result.current(LOCAL)).toBe("Me");
  });

  it("one subscription serves every label, and the listener goes with the last one", async () => {
    function Labels() {
      const label = useParticipantLabel();
      return <span>{label(LOCAL)}</span>;
    }
    const view = render(
      <>
        <Labels />
        <Labels />
      </>,
    );
    await waitFor(() => expect(handlers.get("github-tokens-changed")?.size).toBe(1));
    expect(screen.getAllByText("Me")).toHaveLength(2);
    view.unmount();
    expect(unlistenMock).toHaveBeenCalledTimes(1);
  });
});

describe("GTS-FR-AEQO, CMT-FR-24: the shared writer identity", () => {
  it("resolves to Me with no error when no token is stored, and follows a token change", async () => {
    const { result } = renderHook(() => useSharedCommentIdentity());
    await waitFor(() => expect(result.current.identity).toEqual(LOCAL));
    expect(result.current.identityError).toBeNull();

    identity = () => Promise.reject("github_token_selection_required");
    await tokensChanged();
    await waitFor(() =>
      expect(result.current.identityError).toBe("github_token_selection_required"),
    );

    identity = () => Promise.resolve(GITHUB);
    await tokensChanged();
    await waitFor(() => expect(result.current.identity).toEqual(GITHUB));
    expect(result.current.identityError).toBeNull();
  });
});
